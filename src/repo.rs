//! The repository a command runs in: root, configuration and bank path,
//! observed once per command.
use crate::bail;
use crate::util::{Error, Output, Result, run};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const DEFAULT_BANK: &str = "memory-bank";

pub struct Repo {
    pub root: PathBuf,
    /// Bank directory relative to the root (`agent-memory.bankroot` or `memory-bank`).
    pub bank_rel: String,
    pub bank: PathBuf,
    /// Every `git config` entry, last value wins.
    pub config: BTreeMap<String, String>,
}

/// The nearest ancestor holding `.git` (a directory or a worktree file).
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().ok()?;
    start
        .ancestors()
        .find(|dir| dir.join(".git").symlink_metadata().is_ok())
        .map(Path::to_path_buf)
}

impl Repo {
    /// Discover from `start`; `None` outside a Git repository. One `git` call.
    pub fn discover(start: &Path) -> Result<Option<Repo>> {
        let Some(root) = find_root(start) else {
            return Ok(None);
        };
        let output = git(&root, &["config", "-z", "--list"])?;
        if !output.ok() {
            bail!(
                "Git cannot read this repository's configuration: {}",
                output.error_text()
            );
        }
        let mut config = BTreeMap::new();
        for entry in output.stdout.split(|b| *b == 0).filter(|e| !e.is_empty()) {
            let entry = String::from_utf8_lossy(entry);
            let (key, value) = entry.split_once('\n').unwrap_or((&entry, ""));
            config.insert(key.to_owned(), value.to_owned());
        }
        let bank_rel = match config.get("agent-memory.bankroot") {
            Some(value) => valid_bank_root(value)?.to_owned(),
            None => DEFAULT_BANK.to_owned(),
        };
        Ok(Some(Repo {
            bank: root.join(&bank_rel),
            root,
            bank_rel,
            config,
        }))
    }

    /// Discover from the current directory, requiring a repository.
    pub fn current() -> Result<Repo> {
        let cwd = std::env::current_dir()?;
        Repo::discover(&cwd)?.ok_or_else(|| {
            Error::new("Not inside a Git repository. Run am from your project checkout.")
        })
    }

    pub fn remote_name(&self) -> &str {
        "origin"
    }
    pub fn remote_url(&self) -> Option<&str> {
        self.config.get("remote.origin.url").map(String::as_str)
    }
    pub fn email(&self) -> Option<&str> {
        self.config
            .get("user.email")
            .map(String::as_str)
            .filter(|e| !e.is_empty())
    }
    /// The branch shared memory lands on, if configured.
    pub fn configured_branch(&self) -> Option<&str> {
        self.config
            .get("agent-memory.authoritybranch")
            .map(String::as_str)
    }
    pub fn git(&self, args: &[&str]) -> Result<Output> {
        git(&self.root, args)
    }
    /// Run git and return trimmed stdout, failing with git's own message.
    pub fn git_ok(&self, args: &[&str]) -> Result<String> {
        let output = self.git(args)?;
        if !output.ok() {
            bail!("git {} failed: {}", args.join(" "), output.error_text());
        }
        Ok(output.text())
    }
    /// The bank-relative path in repository form (`memory-bank/x.md`).
    pub fn bank_path(&self, relative: &str) -> String {
        format!("{}/{}", self.bank_rel, relative)
    }
}

pub fn git(root: &Path, args: &[&str]) -> Result<Output> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    run(
        "git",
        &args,
        Some(root),
        &[
            ("GIT_TERMINAL_PROMPT", OsStr::new("0")),
            ("GIT_OPTIONAL_LOCKS", OsStr::new("0")),
        ],
        None,
    )
}

fn valid_bank_root(value: &str) -> Result<&str> {
    let bad = value.is_empty()
        || value.starts_with('/')
        || value
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.'))
        || value.chars().any(|c| c.is_control() || c == '\\');
    if bad {
        bail!(
            "git config agent-memory.bankroot must be a plain repository-relative directory, not {value:?}"
        );
    }
    Ok(value)
}

/// `repo://host/owner/name` for a network remote; `None` for local paths.
pub fn canonical_remote(url: &str) -> Option<String> {
    let url = url.trim();
    let (host, path) = if let Some((_, rest)) = url.split_once("://") {
        if url.starts_with("file://") {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next()?.split(':').next()?;
        (host, path)
    } else if let Some((authority, path)) = url.split_once(':') {
        if authority.contains('/') {
            return None;
        }
        (authority.rsplit('@').next()?, path)
    } else {
        return None;
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let ok = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
    };
    (ok(host) && ok(path) && !path.split('/').any(|p| p == ".." || p.is_empty()))
        .then(|| format!("repo://{}/{}", host.to_ascii_lowercase(), path))
}
