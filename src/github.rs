//! GitHub accounts for git calls that talk to `origin`. When GitHub refuses
//! the user's usual credentials on a github.com HTTPS remote, the call is
//! retried with the other accounts signed in to `gh` (the repository owner's
//! first), and the one that works is kept for the rest of the command. The
//! token reaches only the child git through a credential helper given in its
//! environment: never argv, output, files, git config or the active account.
use crate::bail;
use crate::install::shell_quote;
use crate::repo::Repo;
use crate::util::{Output, Result, run, which};
use std::ffi::{OsStr, OsString};

/// The account a command switched to, and the line telling the user.
pub struct Switch {
    pub login: String,
    pub note: String,
}

/// `owner/name` of a github.com HTTPS remote URL.
pub fn https_repo(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let host = host.rsplit('@').next()?;
    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    (!owner.is_empty() && !name.is_empty() && !name.contains('/')).then(|| path.to_owned())
}

/// Did the remote refuse the credentials (as opposed to the change)?
pub fn refused(error: &str) -> bool {
    [
        " denied to ",
        "returned error: 403",
        "returned error: 401",
        "Authentication failed",
        "could not read Username",
        "could not read Password",
        "Repository not found",
        "Invalid username or",
        "Password authentication is not supported",
    ]
    .iter()
    .any(|text| error.contains(text))
}

fn valid_login(login: &str) -> bool {
    !login.is_empty() && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Accounts signed in to `gh` for github.com: `(login, active)`.
fn accounts() -> Vec<(String, bool)> {
    let query = r#".hosts["github.com"][] | select(.state == "success") | "\(.login) \(.active)""#;
    let args = [
        "auth",
        "status",
        "--hostname",
        "github.com",
        "--json",
        "hosts",
        "--jq",
        query,
    ];
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    let Ok(output) = run("gh", &args, None, &[], None) else {
        return Vec::new();
    };
    output
        .text()
        .lines()
        .filter_map(|line| {
            let (login, active) = line.split_once(' ')?;
            valid_login(login).then(|| (login.to_owned(), active == "true"))
        })
        .collect()
}

/// Environment that makes a child git authenticate to GitHub as `login`:
/// the configured credential helpers are reset and replaced by one that asks
/// `gh` for that account's token. Appended after any `GIT_CONFIG_COUNT`
/// entries the user already set.
pub fn credential_env(login: &str) -> Vec<(String, OsString)> {
    let gh = which("gh").map_or_else(|| "gh".to_owned(), |p| p.to_string_lossy().into_owned());
    let helper = format!(
        "!f() {{ test \"$1\" = get || exit 0; echo username=x-access-token; printf password=; {} auth token --hostname github.com --user {login}; }}; f",
        shell_quote(&gh)
    );
    let start: usize = std::env::var("GIT_CONFIG_COUNT")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let mut env = vec![(
        "GIT_CONFIG_COUNT".to_owned(),
        (start + 2).to_string().into(),
    )];
    for (i, value) in [String::new(), helper].into_iter().enumerate() {
        env.push((
            format!("GIT_CONFIG_KEY_{}", start + i),
            "credential.helper".into(),
        ));
        env.push((format!("GIT_CONFIG_VALUE_{}", start + i), value.into()));
    }
    env
}

/// `login`'s token for a `gh` child (`GH_TOKEN`), read from `gh` itself.
pub fn token(login: &str) -> Option<OsString> {
    let words = ["auth", "token", "--hostname", "github.com", "--user", login];
    let args: Vec<&OsStr> = words.iter().map(OsStr::new).collect();
    let output = run("gh", &args, None, &[], None).ok()?;
    let token = output.text();
    (output.ok() && !token.is_empty()).then(|| token.into())
}

/// `args` failed with `failed` because GitHub refused the credentials: try
/// the other `gh` accounts. Returns the first output that is not a refusal
/// (success, or an ordinary failure such as a moved branch) together with the
/// account that produced it; refuses plainly when no account is let in.
pub fn retry(repo: &Repo, args: &[&str], failed: &Output) -> Result<(Output, Switch)> {
    let error = format!(
        "{}\n{}",
        String::from_utf8_lossy(&failed.stdout),
        failed.error_text()
    );
    let Some(target) = repo.remote_url().and_then(https_repo) else {
        bail!("origin is not a github.com HTTPS remote");
    };
    let owner = target.split('/').next().unwrap_or_default().to_owned();
    let pushing = args.first() == Some(&"push");
    let action = if pushing { "push to" } else { "read" };
    let mut known = accounts();
    let named = error
        .split(" denied to ")
        .nth(1)
        .and_then(|rest| rest.split(['.', '\n', ' ']).next())
        .filter(|login| valid_login(login))
        .map(str::to_owned);
    let first = named.or_else(|| {
        known
            .iter()
            .find(|(_, active)| *active)
            .map(|(login, _)| login.clone())
    });
    let mut refused_by: Vec<String> = first.iter().cloned().collect();
    // Owner first, then the others, the active account (most likely the one
    // just refused) last; the account named in the refusal is skipped.
    known.sort_by_key(|(login, active)| (!login.eq_ignore_ascii_case(&owner), *active));
    for (login, _) in known {
        if refused_by.contains(&login) {
            continue;
        }
        let env = credential_env(&login);
        let env: Vec<(&str, &OsStr)> = env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_os_str()))
            .collect();
        let output = repo.git_with(args, &env, None)?;
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            output.error_text()
        );
        if output.ok() || !refused(&text) {
            let was = first
                .as_deref()
                .map(|f| format!(" ({f} may not {action} it)"))
                .unwrap_or_default();
            let note = format!("Used GitHub account {login} for {target}{was}.");
            return Ok((output, Switch { login, note }));
        }
        refused_by.push(login);
    }
    let who = match refused_by.as_slice() {
        [] => "Your current GitHub credentials may not".to_owned(),
        [login] => format!("GitHub account {login} may not"),
        logins => format!("None of your GitHub accounts ({}) may", logins.join(", ")),
    };
    let missing = if error.contains("Repository not found") {
        " (or it does not exist)"
    } else {
        ""
    };
    let fix = if which("gh").is_none() {
        "install the GitHub CLI (gh) and sign in to an account that has access with `gh auth login`"
    } else {
        "sign in to an account that has access with `gh auth login`"
    };
    bail!(
        "{who} {action} {target}{missing}. Ask the owner of {target} for access, or {fix}. Nothing was changed on GitHub."
    )
}
