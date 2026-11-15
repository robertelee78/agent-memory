//! `am refresh` and `am share`: move memory through Git without touching
//! code or HEAD; only `memory-bank/` entries of the index are updated.
use crate::bail;
use crate::bank::{self, Router};
use crate::repo::Repo;
use crate::util::{Result, io_at, read_optional, run, uuid4, write_atomic};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::Path;

/// Per-worktree record of the remote commit whose memory is already here.
const BASE_REF: &str = "refs/worktree/agent-memory/base";

fn branch(repo: &Repo) -> Result<String> {
    if let Some(branch) = repo.configured_branch() {
        return Ok(branch.to_owned());
    }
    let head = repo.git(&[
        "symbolic-ref",
        "--quiet",
        "--short",
        "refs/remotes/origin/HEAD",
    ])?;
    if head.ok() {
        if let Some(name) = head.text().strip_prefix("origin/") {
            return Ok(name.to_owned());
        }
    }
    let listed = repo.git(&["ls-remote", "--symref", "origin", "HEAD"])?;
    if listed.ok() {
        for line in listed.text().lines() {
            if let Some(rest) = line.strip_prefix("ref: refs/heads/") {
                if let Some((name, _)) = rest.split_once('\t') {
                    return Ok(name.to_owned());
                }
            }
        }
    }
    let local = repo.git(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    Ok(if local.ok() {
        local.text()
    } else {
        "main".into()
    })
}

/// Fetch the shared branch; `None` while the remote branch does not exist.
fn fetch(repo: &Repo, branch: &str) -> Result<Option<String>> {
    if repo.remote_url().is_none() {
        bail!("This repository has no `origin` remote to share memory through.");
    }
    let tracking = format!("refs/remotes/origin/{branch}");
    let fetched = repo.git(&[
        "fetch",
        "--quiet",
        "--no-tags",
        "origin",
        &format!("+refs/heads/{branch}:{tracking}"),
    ])?;
    if !fetched.ok() {
        let message = fetched.error_text();
        if message.contains("couldn't find remote ref") {
            return Ok(None);
        }
        bail!("Could not fetch origin/{branch}: {message}");
    }
    Ok(Some(repo.git_ok(&["rev-parse", "--verify", &tracking])?))
}

/// Blob contents for `rev:path` specs in one `git cat-file --batch`.
fn blobs(repo: &Repo, specs: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
    if specs.is_empty() {
        return Ok(Vec::new());
    }
    let input: String = specs.iter().map(|s| format!("{s}\n")).collect();
    let output = run(
        "git",
        &[OsStr::new("cat-file"), OsStr::new("--batch")],
        Some(&repo.root),
        &[],
        Some(input.as_bytes()),
    )?;
    if !output.ok() {
        bail!("git cat-file failed: {}", output.error_text());
    }
    let raw = output.stdout;
    let mut at = 0;
    let mut result = Vec::new();
    for _ in specs {
        let end = raw[at..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|p| at + p)
            .unwrap_or(raw.len());
        let header = String::from_utf8_lossy(&raw[at..end]).into_owned();
        at = end + 1;
        let fields: Vec<&str> = header.split(' ').collect();
        match fields.as_slice() {
            [_, "blob", size] => {
                let size: usize = size.parse().unwrap_or(0);
                result.push(Some(raw.get(at..at + size).unwrap_or_default().to_vec()));
                at += size + 1;
            }
            _ => result.push(None), // missing, or not a file in that tree
        }
    }
    Ok(result)
}

/// Merge `MEMORY.md`: keep mine, add routes they added, drop routes they removed.
fn merge_router(base: Option<&[u8]>, mine: &[u8], theirs: &[u8]) -> Result<Vec<u8>> {
    let parse = |raw: &[u8]| Router::parse(String::from_utf8_lossy(raw).into_owned());
    let mine = parse(mine)?;
    let theirs = parse(theirs)?;
    let base_paths: BTreeSet<String> = match base {
        Some(raw) => parse(raw)?.routes.into_iter().map(|r| r.path).collect(),
        None => BTreeSet::new(),
    };
    let theirs_paths: BTreeSet<&str> = theirs.routes.iter().map(|r| r.path.as_str()).collect();
    let mine_paths: BTreeSet<&str> = mine.routes.iter().map(|r| r.path.as_str()).collect();
    let stamp = [
        bank::meta_str(&mine.meta, "router-updated").unwrap_or(""),
        bank::meta_str(&theirs.meta, "router-updated").unwrap_or(""),
    ]
    .into_iter()
    .max()
    .unwrap_or("")
    .to_owned();
    // Drop routes they removed.
    let mut text: String = mine
        .text
        .split_inclusive('\n')
        .filter(|line| {
            bank::parse_route(line).is_none_or(|route| {
                !(base_paths.contains(&route.path) && !theirs_paths.contains(route.path.as_str()))
            })
        })
        .collect();
    // Add routes they added.
    for route in &theirs.routes {
        if !mine_paths.contains(route.path.as_str()) && !base_paths.contains(&route.path) {
            text = Router::parse(text)?.with_route(route, &stamp);
        }
    }
    Ok(bank::with_router_time(&text, &stamp).into_bytes())
}

/// Three-way text merge with `git merge-file`; `Err(markers)` on conflict.
fn merge_text(
    repo: &Repo,
    base: &[u8],
    mine: &[u8],
    theirs: &[u8],
) -> Result<Result<Vec<u8>, Vec<u8>>> {
    let dir = std::env::temp_dir().join(format!("am-merge-{}", uuid4()));
    io_at(std::fs::create_dir(&dir), &dir)?;
    let files = [("mine", mine), ("base", base), ("theirs", theirs)];
    for (name, raw) in files {
        io_at(std::fs::write(dir.join(name), raw), &dir)?;
    }
    let output = run(
        "git",
        &[
            OsStr::new("merge-file"),
            OsStr::new("-p"),
            OsStr::new("-L"),
            OsStr::new("yours"),
            OsStr::new("-L"),
            OsStr::new("base"),
            OsStr::new("-L"),
            OsStr::new("shared"),
            dir.join("mine").as_os_str(),
            dir.join("base").as_os_str(),
            dir.join("theirs").as_os_str(),
        ],
        Some(&repo.root),
        &[],
        None,
    );
    let _ = std::fs::remove_dir_all(&dir);
    let output = output?;
    match output.code {
        0 => Ok(Ok(output.stdout)),
        n if n > 0 => Ok(Err(output.stdout)),
        _ => bail!("git merge-file failed: {}", output.error_text()),
    }
}

#[derive(Default)]
struct Merged {
    updated: Vec<String>,
    conflicts: Vec<String>,
}

/// Bring the bank changes in `theirs` (since the recorded base) into the
/// working tree, keeping local edits.
fn merge_into_worktree(repo: &Repo, theirs: &str) -> Result<Merged> {
    let base = repo.git(&["rev-parse", "--verify", "--quiet", BASE_REF])?;
    let base = if base.ok() {
        Some(base.text())
    } else {
        let merge_base = repo.git(&["merge-base", "HEAD", theirs])?;
        merge_base.ok().then(|| merge_base.text())
    };
    let pathspec = format!("{}/", repo.bank_rel);
    let listed = match &base {
        Some(base) => repo.git_ok(&[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            base,
            theirs,
            "--",
            &pathspec,
        ])?,
        None => repo.git_ok(&[
            "ls-tree",
            "-r",
            "--name-only",
            "-z",
            theirs,
            "--",
            &pathspec,
        ])?,
    };
    let paths: Vec<&str> = listed.split('\0').filter(|p| !p.is_empty()).collect();
    let mut specs = Vec::new();
    for path in &paths {
        specs.push(format!("{}:{path}", base.as_deref().unwrap_or("0000000")));
        specs.push(format!("{theirs}:{path}"));
    }
    let contents = blobs(repo, &specs)?;
    let router_path = repo.bank_path(bank::ROUTER);
    let mut merged = Merged::default();
    for (index, path) in paths.iter().enumerate() {
        let base_raw = if base.is_some() {
            contents[index * 2].as_deref()
        } else {
            None
        };
        let theirs_raw = contents[index * 2 + 1].as_deref();
        let file = repo.root.join(path);
        let mine = read_optional(&file)?;
        if mine.as_deref() == theirs_raw {
            continue;
        }
        let result: Option<Vec<u8>> = if mine.as_deref() == base_raw {
            theirs_raw.map(<[u8]>::to_vec)
        } else {
            match (mine.as_deref(), theirs_raw) {
                (Some(mine), Some(theirs)) if *path == router_path => {
                    Some(merge_router(base_raw, mine, theirs)?)
                }
                (Some(mine), Some(theirs)) => {
                    match merge_text(repo, base_raw.unwrap_or_default(), mine, theirs)? {
                        Ok(clean) => Some(clean),
                        Err(marked) => {
                            merged.conflicts.push(path.to_string());
                            Some(marked)
                        }
                    }
                }
                // Deleted on one side and edited on the other: keep the edit.
                (Some(mine), None) => Some(mine.to_vec()),
                (None, Some(theirs)) => Some(theirs.to_vec()),
                (None, None) => None,
            }
        };
        match result {
            Some(raw) => {
                if mine.as_deref() != Some(raw.as_slice()) {
                    if let Some(parent) = file.parent() {
                        io_at(std::fs::create_dir_all(parent), parent)?;
                    }
                    write_atomic(&file, &raw, 0o644)?;
                    merged.updated.push(path.to_string());
                }
            }
            None => {
                if mine.is_some() {
                    io_at(std::fs::remove_file(&file), &file)?;
                    merged.updated.push(path.to_string());
                }
            }
        }
    }
    record_shared(repo, theirs)?;
    Ok(merged)
}

/// Remember `commit` as the shared memory already here, and stage its bank
/// (index only; the working tree keeps local edits) so a later plain
/// `git pull` of the shared branch finds the files already in place.
fn record_shared(repo: &Repo, commit: &str) -> Result<()> {
    repo.git_ok(&["update-ref", BASE_REF, commit])?;
    repo.git_ok(&["reset", "-q", commit, "--", &repo.bank_rel])?;
    Ok(())
}

const STAGED: &str = "The shared memory is staged, so `git pull` works as usual; on a branch with its own commits, commit it first (git commit -m \"Shared memory\").";

fn conflict_help(conflicts: &[String]) -> String {
    format!(
        "Your memory and the shared memory both changed: {}. Edit the marked sections (<<<<<<< yours … >>>>>>> shared), then run am share again.",
        conflicts.join(", ")
    )
}

/// `am refresh`.
pub fn refresh() -> Result<String> {
    let repo = Repo::current()?;
    let branch = branch(&repo)?;
    let Some(theirs) = fetch(&repo, &branch)? else {
        return Ok(format!(
            "origin/{branch} does not exist yet; nothing to refresh."
        ));
    };
    let merged = merge_into_worktree(&repo, &theirs)?;
    let mut out = if merged.updated.is_empty() {
        format!("Memory is up to date with origin/{branch}.")
    } else {
        format!(
            "Brought {} memory file(s) from origin/{branch}:\n  {}\n{STAGED}",
            merged.updated.len(),
            merged.updated.join("\n  ")
        )
    };
    if !merged.conflicts.is_empty() {
        out.push('\n');
        out.push_str(&conflict_help(&merged.conflicts));
    }
    Ok(out)
}

fn has_markers(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if std::fs::read_to_string(&path)
                .is_ok_and(|t| t.contains("\n<<<<<<< ") || t.starts_with("<<<<<<< "))
            {
                found.push(path.display().to_string());
            }
        }
    }
    found
}

/// `am share`: commit only the bank on top of the shared branch and push it.
pub fn share(message: Option<&str>) -> Result<String> {
    let repo = Repo::current()?;
    if !bank::Bank::new(&repo.bank).exists() {
        bail!("No memory bank at {}/. Run `am init` first.", repo.bank_rel);
    }
    let marked = has_markers(&repo.bank);
    if !marked.is_empty() {
        bail!("{}", conflict_help(&marked));
    }
    let branch = branch(&repo)?;
    for _attempt in 0..3 {
        let theirs = fetch(&repo, &branch)?;
        if let Some(theirs) = &theirs {
            let merged = merge_into_worktree(&repo, theirs)?;
            if !merged.conflicts.is_empty() {
                bail!("{}", conflict_help(&merged.conflicts));
            }
        }
        let Some(commit) = commit_bank(&repo, theirs.as_deref(), message)? else {
            return Ok(format!(
                "Nothing new to share; origin/{branch} has all of this memory."
            ));
        };
        let pushed = repo.git(&[
            "push",
            "--porcelain",
            "--no-verify",
            "origin",
            &format!("{commit}:refs/heads/{branch}"),
        ])?;
        let short = &commit[..commit.len().min(10)];
        if pushed.ok() {
            record_shared(&repo, &commit)?;
            return Ok(format!(
                "Shared memory to origin/{branch} (commit {short}).\n{STAGED}"
            ));
        }
        let error = format!("{}\n{}", pushed.text(), pushed.error_text());
        if error.contains("fetch first")
            || error.contains("non-fast-forward")
            || error.contains("stale info")
        {
            continue; // Someone shared meanwhile: merge again and retry.
        }
        if ["protected", "GH006", "GH013", "declined", "denied", "rule"]
            .iter()
            .any(|word| error.contains(word))
        {
            return propose(&repo, &branch, &commit);
        }
        bail!("Push to origin/{branch} failed: {}", error.trim());
    }
    bail!("origin/{branch} kept moving while sharing; run am share again.")
}

/// A commit holding the shared tree with this worktree's bank, or `None`
/// when that equals the shared tree.
fn commit_bank(repo: &Repo, theirs: Option<&str>, message: Option<&str>) -> Result<Option<String>> {
    let index = std::env::temp_dir().join(format!("am-index-{}", uuid4()));
    let env = [("GIT_INDEX_FILE", index.as_os_str())];
    let git = |args: &[&str]| -> Result<String> {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        let output = run("git", &args, Some(&repo.root), &env, None)?;
        if !output.ok() {
            bail!(
                "git failed while preparing the memory commit: {}",
                output.error_text()
            );
        }
        Ok(output.text())
    };
    let result = (|| {
        match theirs {
            Some(theirs) => git(&["read-tree", theirs])?,
            None => git(&["read-tree", "--empty"])?,
        };
        git(&["add", "-A", "--", &repo.bank_rel])?;
        let tree = git(&["write-tree"])?;
        if let Some(theirs) = theirs {
            if git(&["rev-parse", &format!("{theirs}^{{tree}}")])? == tree {
                return Ok(None);
            }
        }
        let changed = match theirs {
            Some(theirs) => git(&["diff", "--cached", "--name-only", theirs])?,
            None => git(&["ls-files"])?,
        };
        let count = changed.lines().count();
        let subject = message
            .map(str::to_owned)
            .unwrap_or_else(|| format!("memory: update {count} file(s) in {}", repo.bank_rel));
        let mut args = vec!["commit-tree", tree.as_str(), "-m", subject.as_str()];
        if let Some(theirs) = theirs {
            args.extend(["-p", theirs]);
        }
        git(&args).map(Some)
    })();
    let _ = std::fs::remove_file(&index);
    result
}

/// The branch is protected: push a memory branch and open a pull request.
fn propose(repo: &Repo, branch: &str, commit: &str) -> Result<String> {
    let project = bank::Bank::new(&repo.bank)
        .router()?
        .project_id()
        .unwrap_or("memory")
        .to_owned();
    let head = format!("agent-memory/{project}/{}", &commit[..commit.len().min(12)]);
    repo.git_ok(&[
        "push",
        "--porcelain",
        "--no-verify",
        "origin",
        &format!("{commit}:refs/heads/{head}"),
    ])?;
    if crate::util::which("gh").is_none() {
        return Ok(format!(
            "origin/{branch} is protected, so memory was pushed to branch {head}. Open a pull request from it into {branch} (install the gh CLI to have am open it)."
        ));
    }
    let output = run(
        "gh",
        &[
            OsStr::new("pr"),
            OsStr::new("create"),
            OsStr::new("--base"),
            OsStr::new(branch),
            OsStr::new("--head"),
            OsStr::new(&head),
            OsStr::new("--title"),
            OsStr::new("Share project memory"),
            OsStr::new("--body"),
            OsStr::new(
                "Memory-only change prepared by `am share`: it touches only the memory bank.",
            ),
        ],
        Some(&repo.root),
        &[("GH_PROMPT_DISABLED", OsStr::new("1"))],
        None,
    )?;
    if output.ok() {
        Ok(format!(
            "origin/{branch} is protected; opened a pull request for the memory: {}",
            output.text()
        ))
    } else {
        Ok(format!(
            "origin/{branch} is protected; memory was pushed to {head}, but gh could not open the pull request ({}). Open it manually.",
            output.error_text()
        ))
    }
}
