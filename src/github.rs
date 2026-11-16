//! GitHub accounts for git calls that talk to `origin`. When GitHub refuses
//! a call whose destination (after git's URL rewrites) is a github.com HTTPS
//! repository, the call is retried with the accounts signed in to `gh` (the
//! repository owner's first), and the one that works is kept for the rest of
//! the command. The token reaches only the child git, through a credential
//! helper given in its environment and scoped to https://github.com: never
//! argv, output, files, git config, another host or the active gh account.
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

/// What the user's own credentials are called when GitHub does not name them.
const CURRENT: &str = "your current Git login";

/// `owner/name` of a github.com HTTPS URL.
fn https_repo(url: &str) -> Option<String> {
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

/// Standard output and error of a git call, for matching git's messages.
pub fn said(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        output.error_text()
    )
}

fn valid_login(login: &str) -> bool {
    !login.is_empty() && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Run `gh` with colours off, so its output can be read as plain text.
fn gh(args: &[&str]) -> Option<Output> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    let plain = [
        ("NO_COLOR", OsStr::new("1")),
        ("CLICOLOR", OsStr::new("0")),
        ("CLICOLOR_FORCE", OsStr::new("0")),
        ("GH_FORCE_TTY", OsStr::new("")),
    ];
    run("gh", &args, None, &plain, None).ok()
}

/// Accounts signed in to `gh` for github.com. `--json` needs gh 2.81 or
/// newer; older ones (2.40+, the first with several accounts) print
/// "Logged in to github.com account NAME" lines instead.
fn accounts() -> Vec<String> {
    let query = r#".hosts["github.com"][] | select(.state == "success") | .login"#;
    let json = [
        "auth",
        "status",
        "--hostname",
        "github.com",
        "--json",
        "hosts",
        "--jq",
        query,
    ];
    let logins: Vec<String> = match gh(&json) {
        Some(output) if output.ok() => output.text().lines().map(str::to_owned).collect(),
        Some(_) => match gh(&["auth", "status", "--hostname", "github.com"]) {
            Some(output) => said(&output)
                .lines()
                .filter_map(|line| line.split("Logged in to github.com account ").nth(1))
                .filter_map(|rest| rest.split_whitespace().next())
                .map(str::to_owned)
                .collect(),
            None => Vec::new(),
        },
        None => Vec::new(),
    };
    logins.into_iter().filter(|l| valid_login(l)).collect()
}

/// Environment that makes a child git authenticate to https://github.com as
/// `login`: the helpers configured for github.com are reset and replaced by
/// one that asks `gh` for that account's token. Other hosts keep the user's
/// helpers. Appended after any `GIT_CONFIG_COUNT` entries the user set.
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
            "credential.https://github.com.helper".into(),
        ));
        env.push((format!("GIT_CONFIG_VALUE_{}", start + i), value.into()));
    }
    env
}

/// `login`'s token for a `gh` child (`GH_TOKEN`), read from `gh` itself.
pub fn token(login: &str) -> Option<OsString> {
    let output = gh(&["auth", "token", "--hostname", "github.com", "--user", login])?;
    let token = output.text();
    (output.ok() && !token.is_empty()).then(|| token.into())
}

/// Where `args` sends git, after git's URL rewrites: the push URL for a
/// push, the fetch URL otherwise.
fn destination(repo: &Repo, args: &[&str]) -> Option<String> {
    let push = args.first() == Some(&"push");
    let get: &[&str] = if push {
        &["remote", "get-url", "--push", "origin"]
    } else {
        &["remote", "get-url", "origin"]
    };
    let output = repo.git(get).ok()?;
    output.ok().then(|| output.text())
}

/// `args` failed with `failed` because the remote refused the credentials
/// (this command's own `supplied` account, if any). When the call goes to a
/// github.com HTTPS repository, try the other `gh` accounts, owner first.
/// Returns the first output that is not a refusal (success, or an ordinary
/// failure such as a moved branch) and the account that produced it; `None`
/// when the destination is not GitHub over HTTPS; refuses plainly when no
/// account is let in.
pub fn retry(
    repo: &Repo,
    args: &[&str],
    failed: &Output,
    supplied: Option<&str>,
) -> Result<Option<(Output, Switch)>> {
    let Some(target) = destination(repo, args).as_deref().and_then(https_repo) else {
        return Ok(None);
    };
    let error = said(failed);
    let owner = target.split('/').next().unwrap_or_default().to_owned();
    let action = if args.first() == Some(&"push") {
        "push to"
    } else {
        "read"
    };
    // Who was refused: the account GitHub names, else the one this command
    // supplied; otherwise the user's own Git login, which may be anything.
    let named = error
        .split(" denied to ")
        .nth(1)
        .and_then(|rest| rest.split(['.', '\n', ' ']).next())
        .filter(|login| valid_login(login))
        .or(supplied)
        .map(str::to_owned);
    let mut refused_by = vec![named.clone().unwrap_or_else(|| CURRENT.to_owned())];
    let mut known = accounts();
    known.sort_by_key(|login| !login.eq_ignore_ascii_case(&owner));
    for login in &known {
        if named.as_deref() == Some(login) || supplied == Some(login) {
            continue;
        }
        let env = credential_env(login);
        let env: Vec<(&str, &OsStr)> = env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_os_str()))
            .collect();
        let output = repo.git_with(args, &env, None)?;
        if output.ok() || !refused(&said(&output)) {
            let note = format!(
                "Used GitHub account {login} for {target} ({} may not {action} it).",
                refused_by[0]
            );
            let login = login.clone();
            return Ok(Some((output, Switch { login, note })));
        }
        refused_by.push(login.clone());
    }
    let who = match refused_by.as_slice() {
        [one] if one == CURRENT => "Your current Git login may not".to_owned(),
        [one] => format!("GitHub account {one} may not"),
        many => format!("None of your GitHub logins ({}) may", many.join(", ")),
    };
    let missing = if error.contains("Repository not found") {
        " (or it does not exist)"
    } else {
        ""
    };
    let fix = if which("gh").is_none() {
        "install the GitHub CLI (gh) and sign in to an account that has access with `gh auth login`"
    } else if known.is_empty() {
        "sign in to an account that has access with `gh auth login` (am chooses among the github.com accounts gh 2.40 or newer lists)"
    } else {
        "sign in to an account that has access with `gh auth login`"
    };
    bail!(
        "{who} {action} {target}{missing}. Ask the owner of {target} for access, or {fix}. Nothing was changed on GitHub."
    )
}
