//! `am refresh` and `am share`: move memory through Git without touching
//! code; only `memory-bank/` entries of the index are updated, and HEAD only
//! moves over memory-only commits (see `settle`).
use crate::bail;
use crate::bank::{self, Router};
use crate::repo::Repo;
use crate::util::{Result, io_at, lock, read_optional, run, uuid4, write_atomic};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::Path;

/// Per-worktree record of the remote commit whose memory is already here.
const BASE_REF: &str = "refs/worktree/agent-memory/base";

/// Subject of the memory-only commit `settle` makes for rebasing pulls.
const MEMORY_COMMIT: &str = "Shared memory";

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
                    // Remember it like `git clone` does, so later runs skip the network.
                    let _ = repo.git(&[
                        "symbolic-ref",
                        "refs/remotes/origin/HEAD",
                        &format!("refs/remotes/origin/{name}"),
                    ]);
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

/// Object ids for revisions or `rev:path` specs in one `git cat-file --batch-check`.
fn object_ids(repo: &Repo, specs: &[String]) -> Result<Vec<Option<String>>> {
    let input: String = specs.iter().map(|s| format!("{s}\n")).collect();
    let output = repo.git_with(&["cat-file", "--batch-check"], &[], Some(input.as_bytes()))?;
    if !output.ok() {
        bail!("git cat-file failed: {}", output.error_text());
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(text
        .lines()
        .map(|line| {
            let fields: Vec<&str> = line.split(' ').collect();
            match fields.as_slice() {
                [oid, _, size] if size.parse::<u64>().is_ok() => Some((*oid).to_owned()),
                _ => None,
            }
        })
        .collect())
}

/// What the shared branch holds now and what this worktree last merged.
struct Shared {
    branch: String,
    /// The fetched shared commit; `None` while the remote branch is absent.
    theirs: Option<String>,
    /// The shared commit whose memory is already here.
    base: Option<String>,
    /// Whether HEAD's bank tree differs from the shared commit's.
    head_differs: bool,
}

/// Fetch the shared branch, then resolve it and the recorded base together.
fn observe(repo: &Repo) -> Result<Shared> {
    if repo.remote_url().is_none() {
        bail!("This repository has no `origin` remote to share memory through.");
    }
    let branch = branch(repo)?;
    let tracking = format!("refs/remotes/origin/{branch}");
    let fetched = repo.git(&[
        "fetch",
        "--quiet",
        "--no-tags",
        "origin",
        &format!("+refs/heads/{branch}:{tracking}"),
    ])?;
    let exists = fetched.ok();
    if !exists && !fetched.error_text().contains("couldn't find remote ref") {
        bail!("Could not fetch origin/{branch}: {}", fetched.error_text());
    }
    let bank = &repo.bank_rel;
    let ids = object_ids(
        repo,
        &[
            format!("{tracking}:{bank}"),
            format!("HEAD:{bank}"),
            tracking,
            BASE_REF.to_owned(),
        ],
    )?;
    Ok(Shared {
        theirs: if exists { ids[2].clone() } else { None },
        base: ids.get(3).cloned().flatten(),
        head_differs: ids[0] != ids[1],
        branch,
    })
}

/// Blob contents for `rev:path` specs in one `git cat-file --batch`.
fn blobs(repo: &Repo, specs: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
    if specs.is_empty() {
        return Ok(Vec::new());
    }
    let input: String = specs.iter().map(|s| format!("{s}\n")).collect();
    let output = repo.git_with(&["cat-file", "--batch"], &[], Some(input.as_bytes()))?;
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

const ROWS: &str = "<!-- am: routes -->";

/// `MEMORY.md` without its route rows (one marker where they stand) and
/// with the router time blanked: the part merged as ordinary text.
fn router_skeleton(text: &str) -> String {
    let mut out = String::new();
    let mut marked = false;
    let mut in_front = false;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if line.trim_end() == "---" {
            in_front = index == 0; // Opens on the first line, closes on the next.
        }
        if bank::parse_route(line).is_some() {
            if !marked {
                out.push_str(ROWS);
                out.push('\n');
                marked = true;
            }
        } else if in_front && line.starts_with("router-updated:") {
            out.push_str("router-updated: \"\"\n");
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Three-way merge of `MEMORY.md`: rows by path, the rest as text. Returns
/// the text and what both sides changed differently (yours kept).
fn merge_router(
    repo: &Repo,
    base: Option<&[u8]>,
    mine: &[u8],
    theirs: &[u8],
) -> Result<(Vec<u8>, Vec<String>)> {
    let parse = |raw: &[u8]| Router::parse(String::from_utf8_lossy(raw).into_owned());
    let rows = |router: &Router| -> BTreeMap<String, String> {
        router
            .routes
            .iter()
            .map(|r| (r.path.clone(), r.row()))
            .collect()
    };
    let mine = parse(mine)?;
    let theirs = parse(theirs)?;
    let base_rows = match base {
        Some(raw) => rows(&parse(raw)?),
        None => BTreeMap::new(),
    };
    let their_rows = rows(&theirs);
    let mine_paths: BTreeSet<String> = mine.routes.iter().map(|r| r.path.clone()).collect();
    let stamp = [
        bank::meta_str(&mine.meta, "router-updated").unwrap_or(""),
        bank::meta_str(&theirs.meta, "router-updated").unwrap_or(""),
    ]
    .into_iter()
    .max()
    .unwrap_or("")
    .to_owned();
    let mut kept = Vec::new();
    let mut text = String::new();
    for line in mine.text.split_inclusive('\n') {
        let Some(route) = bank::parse_route(line) else {
            text.push_str(line);
            continue;
        };
        let row = route.row();
        match (base_rows.get(&route.path), their_rows.get(&route.path)) {
            // They removed a row you left alone: drop it.
            (Some(b), None) if *b == row => continue,
            // Only they changed it: take theirs.
            (Some(b), Some(t)) if *b == row && *t != row => {
                text.push_str(t);
                text.push('\n');
                continue;
            }
            (b, Some(t)) if *t != row && b != Some(t) => kept.push(route.path.clone()),
            _ => {}
        }
        text.push_str(line);
    }
    // Rows they added, or edited after you removed them.
    for route in &theirs.routes {
        let removed_unchanged = base_rows.get(&route.path) == Some(&route.row());
        if !mine_paths.contains(&route.path) && !removed_unchanged {
            text = Router::parse(text)?.with_route(route, &stamp);
        }
    }
    // Frontmatter and text around the table.
    let lossy = |raw: &[u8]| String::from_utf8_lossy(raw).into_owned();
    let base_skel = router_skeleton(&base.map(lossy).unwrap_or_default());
    let mine_skel = router_skeleton(&mine.text);
    let their_skel = router_skeleton(&theirs.text);
    let skeleton = if their_skel == mine_skel || their_skel == base_skel {
        None
    } else if mine_skel == base_skel {
        Some(their_skel)
    } else {
        let merged = merge_text(
            repo,
            bank::ROUTER,
            base_skel.as_bytes(),
            mine_skel.as_bytes(),
            their_skel.as_bytes(),
        )?;
        match merged {
            Ok(clean) => Some(lossy(&clean)),
            Err(_) => {
                kept.push("text outside the routes table".into());
                None
            }
        }
    };
    if let Some(skeleton) = skeleton {
        let rows: Vec<&str> = text
            .lines()
            .filter(|l| bank::parse_route(l).is_some())
            .collect();
        let rebuilt = if skeleton.lines().any(|l| l == ROWS) {
            skeleton
                .split_inclusive('\n')
                .map(|l| {
                    if l.trim_end() == ROWS {
                        rows.iter().map(|r| format!("{r}\n")).collect()
                    } else {
                        l.to_owned()
                    }
                })
                .collect()
        } else {
            let mut rebuilt = skeleton;
            for route in rows.iter().filter_map(|r| bank::parse_route(r)) {
                rebuilt = Router::parse(rebuilt)?.with_route(&route, &stamp);
            }
            rebuilt
        };
        text = rebuilt;
    }
    Ok((bank::with_router_time(&text, &stamp).into_bytes(), kept))
}

/// Three-way text merge with `git merge-file`; `Err(markers)` on conflict.
fn merge_text(
    repo: &Repo,
    path: &str,
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
        1..=127 => Ok(Err(output.stdout)), // the number of conflicts
        _ => bail!(
            "git could not merge {path} ({}); nothing was changed. Merge it by hand, then run am again.",
            output.error_text()
        ),
    }
}

#[derive(Default)]
struct Merged {
    updated: Vec<String>,
    conflicts: Vec<String>,
    /// MEMORY.md rows both sides changed; yours were kept.
    kept_rows: Vec<String>,
}

/// Bank paths whose index content exists nowhere else: not in HEAD, not in
/// the working tree and not in one of the `shared` commits.
fn staged_only(repo: &Repo, command: &str, shared: &[&str]) -> Result<Vec<String>> {
    let pathspec = format!("{}/", repo.bank_rel);
    let output = repo.git(&[
        "status",
        "--porcelain=v2",
        "-z",
        "--untracked-files=no",
        "--no-renames",
        "--",
        &pathspec,
    ])?;
    if !output.ok() {
        bail!("git status failed: {}", output.error_text());
    }
    let mut candidates = Vec::new();
    for record in output.stdout.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = String::from_utf8_lossy(record);
        if record.starts_with("u ") {
            bail!(
                "Memory has an unfinished Git merge ({record}); finish it, then run am {command} again."
            );
        }
        let fields: Vec<&str> = record.splitn(9, ' ').collect();
        if let ["1", xy, _, _, _, _, _, index_oid, path] = fields.as_slice() {
            let mut flags = xy.chars();
            if flags.next() != Some('.') && flags.next() != Some('.') {
                candidates.push((path.to_string(), index_oid.to_string()));
            }
        }
    }
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let specs: Vec<String> = candidates
        .iter()
        .flat_map(|(path, _)| shared.iter().map(move |rev| format!("{rev}:{path}")))
        .collect();
    let ids = object_ids(repo, &specs)?;
    Ok(candidates
        .iter()
        .enumerate()
        .filter(|(i, (_, oid))| {
            !ids[i * shared.len()..(i + 1) * shared.len()]
                .iter()
                .any(|id| id.as_deref() == Some(oid.as_str()))
        })
        .map(|(_, (path, _))| path.clone())
        .collect())
}

/// Refuse before anything changes when the index holds the only copy.
fn check_staged(repo: &Repo, command: &str, shared: &[&str]) -> Result<()> {
    let lost = staged_only(repo, command, shared)?;
    if !lost.is_empty() {
        bail!(
            "You staged memory changes that exist only in the Git index: {}. Commit them, or `git add`/`git restore --staged` them, then run am {command} again.",
            lost.join(", ")
        );
    }
    Ok(())
}

/// Bring the bank changes in `theirs` since `base` into the working tree,
/// keeping local edits. The caller holds the bank lock. Nothing is written
/// unless every file merges or gets conflict markers.
fn merge_into_worktree(repo: &Repo, theirs: &str, base: Option<&str>) -> Result<Merged> {
    let base = match base {
        Some(base) => Some(base.to_owned()),
        None => {
            let merge_base = repo.git(&["merge-base", "HEAD", theirs])?;
            merge_base.ok().then(|| merge_base.text())
        }
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
    let mut merged = Merged::default();
    if paths.is_empty() {
        return Ok(merged);
    }
    let mut specs = Vec::new();
    for path in &paths {
        specs.push(format!("{}:{path}", base.as_deref().unwrap_or("0000000")));
        specs.push(format!("{theirs}:{path}"));
    }
    let contents = blobs(repo, &specs)?;
    let router_path = repo.bank_path(bank::ROUTER);
    let mut writes = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        repo.refuse_links(path)?;
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
                    let (text, kept) = merge_router(repo, base_raw, mine, theirs)?;
                    merged.kept_rows.extend(kept);
                    Some(text)
                }
                (Some(mine), Some(theirs)) => {
                    match merge_text(repo, path, base_raw.unwrap_or_default(), mine, theirs)? {
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
        if result != mine {
            writes.push((path.to_string(), mine, result));
        }
    }
    for (path, mine, result) in writes {
        let file = repo.root.join(&path);
        // The lock keeps am writers out; this catches an editor's save.
        if read_optional(&file)? != mine {
            bail!("{path} changed while am was merging it; run am again.");
        }
        match result {
            Some(raw) => {
                if let Some(parent) = file.parent() {
                    io_at(std::fs::create_dir_all(parent), parent)?;
                }
                write_atomic(&file, &raw, 0o644)?;
            }
            None => io_at(std::fs::remove_file(&file), &file)?,
        }
        merged.updated.push(path);
    }
    Ok(merged)
}

/// Remember `commit` as the shared memory already here, and stage its bank
/// (index only; the working tree keeps local edits); `settle` then leaves it
/// so that a plain `git pull` works.
fn record_shared(repo: &Repo, commit: &str) -> Result<()> {
    repo.git_ok(&["update-ref", BASE_REF, commit])?;
    repo.git_ok(&["reset", "-q", commit, "--", &repo.bank_rel])?;
    Ok(())
}

/// Stage `commit`'s bank under the bank lock, unless the index holds the
/// only copy of something the user staged (checked right before the reset),
/// then settle it for the next `git pull`. Returns what to tell the user.
fn stage_shared(
    repo: &Repo,
    shared: &str,
    commit: &str,
    known: &[&str],
    previous: Option<&str>,
) -> Result<String> {
    let _lock = lock_bank(repo)?;
    let mut recent = known.to_vec();
    recent.push(commit);
    let lost = staged_only(repo, "share", &recent).unwrap_or_else(|e| vec![e.0]);
    if !lost.is_empty() {
        // The recorded base stays as it is, matching what is staged.
        return Ok(format!(
            "The shared memory was not staged, to keep the changes you staged meanwhile: {}.",
            lost.join(", ")
        ));
    }
    record_shared(repo, commit)?;
    settle_note(repo, shared, commit, previous)
}

/// Does the index's bank equal `commit`'s?
fn index_holds(repo: &Repo, commit: &str) -> Result<bool> {
    let bank = format!("{}/", repo.bank_rel);
    Ok(repo
        .git(&["diff", "--cached", "--quiet", commit, "--", &bank])?
        .ok())
}

/// Does `git pull` on `branch` rebase (`branch.<name>.rebase`, else `pull.rebase`)?
fn pull_rebases(repo: &Repo, branch: &str) -> bool {
    repo.config
        .get(&format!("branch.{branch}.rebase"))
        .or_else(|| repo.config.get("pull.rebase"))
        .is_some_and(|v| !["false", "no", "off", "0"].contains(&v.to_ascii_lowercase().as_str()))
}

/// Where the shared memory staged by `record_shared` was left.
enum Settled {
    /// HEAD already holds it.
    Clean,
    /// The branch moved to the shared commit over memory-only commits.
    Moved(String),
    /// A memory-only commit on the branch, for a rebasing `git pull`.
    Committed(String),
    /// Still staged; whether a plain `git pull` accepts that.
    Staged { pull_works: bool },
}

/// How `git pull` behaves on the current branch.
#[derive(Default)]
struct Pull {
    /// It rebases (`branch.<name>.rebase`, else `pull.rebase`).
    rebase: bool,
    /// `pull.ff=only`: it refuses anything but a fast-forward.
    ff_only: bool,
    /// The branch has an upstream to pull from.
    upstream: bool,
}

/// After `record_shared(commit)`, leave a state a plain `git pull` accepts.
/// When the branch tracks the shared branch and only memory lies between
/// HEAD and `commit`, move the branch there. When `git pull` merges and HEAD
/// is behind `commit` (with `pull.ff=only`: HEAD can still fast-forward to
/// its upstream), or the branch has no upstream, keep the memory staged: a
/// fast-forward takes it.
/// Otherwise, when the branch's own commits leave the bank alone, commit the
/// shared bank on its own: a rebase onto commits that hold it drops that
/// commit, a merge finds the same files on both sides. Such a commit made
/// earlier for `previous` and not yet upstream is replaced, not stacked.
fn settle(
    repo: &Repo,
    shared: &str,
    commit: &str,
    previous: Option<&str>,
) -> Result<(Settled, Pull)> {
    let stuck = Settled::Staged { pull_works: false };
    let symbolic = repo.git(&["symbolic-ref", "-q", "HEAD"])?;
    let Some(name) = symbolic
        .ok()
        .then(|| symbolic.text())
        .and_then(|r| r.strip_prefix("refs/heads/").map(str::to_owned))
    else {
        return Ok((stuck, Pull::default())); // detached HEAD
    };
    let config = |key: &str| repo.config.get(&format!("branch.{name}.{key}"));
    let rebase = pull_rebases(repo, &name);
    let pull = Pull {
        rebase,
        ff_only: repo
            .config
            .get("pull.ff")
            .is_some_and(|v| v.eq_ignore_ascii_case("only")),
        upstream: config("merge").is_some(),
    };
    let none = "0".repeat(40); // names no object
    let fork = repo.git(&["merge-base", "HEAD", commit])?;
    let fork = if fork.ok() { fork.text() } else { none.clone() };
    let bank = &repo.bank_rel;
    let ids = object_ids(
        repo,
        &[
            "HEAD".to_owned(),
            "MERGE_HEAD".to_owned(),
            "CHERRY_PICK_HEAD".to_owned(),
            "REVERT_HEAD".to_owned(),
            "HEAD^2".to_owned(),
            format!("HEAD:{bank}"),
            format!("{commit}:{bank}"),
            format!("{fork}:{bank}"),
            format!("HEAD^:{bank}"),
            format!("{}:{bank}", previous.unwrap_or(&none)),
        ],
    )?;
    let Some(head) = ids[0].clone() else {
        return Ok((stuck, pull)); // no commit yet
    };
    if ids[1..4].iter().any(Option::is_some) {
        return Ok((stuck, pull)); // a merge, cherry-pick or revert is in progress
    }
    let (head_bank, fork_bank, parent_bank) = (&ids[5], &ids[7], &ids[8]);
    if head_bank == &ids[6] {
        return Ok((Settled::Clean, pull));
    }
    let prefix = format!("{bank}/");
    let only_memory = |from: &str, to: &str| -> Result<bool> {
        let changed = repo.git_ok(&["diff", "--name-only", "-z", "--no-renames", from, to])?;
        Ok(changed
            .split('\0')
            .filter(|p| !p.is_empty())
            .all(|p| p.starts_with(&prefix)))
    };
    let tracks = config("remote").is_some_and(|r| r == "origin")
        && config("merge").is_some_and(|m| *m == format!("refs/heads/{shared}"));
    if tracks && fork == head && only_memory(&head, commit)? {
        let moved = repo.git(&[
            "update-ref",
            "-m",
            &format!("am: memory-only commits from origin/{shared}"),
            &format!("refs/heads/{name}"),
            commit,
            &head,
        ])?;
        if moved.ok() {
            return Ok((Settled::Moved(name), pull));
        }
    }
    // With `pull.ff=only` a merging pull only fast-forwards: the staged
    // memory suits that while HEAD can still fast-forward to its upstream;
    // once they diverged, a memory commit lets `git pull --no-ff` merge cleanly.
    let ff_possible = || -> Result<bool> {
        Ok(repo
            .git(&["merge-base", "--is-ancestor", "HEAD", "@{upstream}"])?
            .ok())
    };
    let stage = if pull.ff_only {
        ff_possible()?
    } else {
        fork == head
    };
    if !pull.upstream || (!rebase && stage) {
        return Ok((Settled::Staged { pull_works: true }, pull));
    }
    let parent = if head_bank == fork_bank {
        head.clone()
    } else if ids[9].is_some()
        && head_bank == &ids[9]
        && parent_bank == fork_bank
        && ids[4].is_none()
        && !repo
            .git(&["merge-base", "--is-ancestor", "HEAD", "@{upstream}"])?
            .ok()
        && only_memory(&format!("{head}^"), &head)?
    {
        format!("{head}^")
    } else {
        return Ok((stuck, pull)); // the branch has its own memory commits
    };
    let made = temp_index(repo, |git| {
        git(&["read-tree", &parent])?;
        git(&["reset", "-q", commit, "--", bank])?;
        let tree = git(&["write-tree"])?;
        let message = format!(
            "{MEMORY_COMMIT}\n\nOnly {bank}/, as on origin/{shared}, committed by am so that git pull\nworks: a rebase onto commits that hold this memory drops it, a merge\nfinds the same files on both sides."
        );
        git(&["commit-tree", &tree, "-p", &parent, "-m", &message])
    });
    let Ok(new) = made else {
        return Ok((stuck, pull));
    };
    let moved = repo.git(&[
        "update-ref",
        "-m",
        &format!("am: shared memory from origin/{shared}"),
        &format!("refs/heads/{name}"),
        &new,
        &head,
    ])?;
    Ok((
        if moved.ok() {
            Settled::Committed(name)
        } else {
            stuck
        },
        pull,
    ))
}

/// `settle`, in words.
fn settle_note(repo: &Repo, shared: &str, commit: &str, previous: Option<&str>) -> Result<String> {
    let (settled, pull) = settle(repo, shared, commit, previous)?;
    let bank = &repo.bank_rel;
    let done = match &settled {
        Settled::Clean => return Ok(String::new()),
        Settled::Staged { pull_works: false } => {
            return Ok(
                "The shared memory is staged; commit or stash your changes before `git pull`."
                    .into(),
            );
        }
        Settled::Moved(branch) => {
            format!("{branch} moved to the shared memory commit (only {bank}/ changed).")
        }
        Settled::Committed(branch) => format!(
            "The shared memory is committed on {branch} as one commit that changes only {bank}/."
        ),
        Settled::Staged { pull_works: true } => "The shared memory is staged.".into(),
    };
    if !pull.upstream {
        return Ok(format!(
            "{done} This branch has no upstream branch, so there is nothing for `git pull` to pull."
        ));
    }
    // A rebasing pull refuses uncommitted changes to tracked files, any file.
    let mut dirty = None;
    if pull.rebase {
        let status = repo.git(&[
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=no",
            "--no-renames",
        ])?;
        if !status.ok() {
            bail!("git status failed: {}", status.error_text());
        }
        let status = String::from_utf8_lossy(&status.stdout);
        let paths: Vec<&str> = status
            .split('\0')
            .filter_map(|entry| entry.get(3..))
            .collect();
        if !paths.is_empty() {
            let prefix = format!("{bank}/");
            dirty = Some(paths.iter().any(|p| p.starts_with(&prefix)));
        }
    }
    // pull.ff=only refuses once HEAD (with any memory commit) and its
    // upstream have both moved.
    let mut command = None;
    if pull.ff_only {
        let counts = repo.git(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])?;
        let diverged = counts.ok()
            && counts
                .text()
                .split_whitespace()
                .all(|n| n.parse::<u64>().is_ok_and(|n| n > 0));
        if diverged {
            command = Some(if pull.rebase {
                "git pull --rebase"
            } else {
                "git pull --no-ff"
            });
        }
    }
    let sentence = match (dirty, command) {
        (None, None) => "`git pull` works as usual.".to_owned(),
        (None, Some(command)) => format!(
            "Pull with `{command}`: with pull.ff=only a plain `git pull` refuses, as this branch and its upstream have both moved."
        ),
        (Some(memory), command) => format!(
            "You have uncommitted changes{}; git will ask you to commit or stash them before a rebasing pull.{}",
            if memory {
                " (memory edits not shared yet: am share shares them)"
            } else {
                ""
            },
            command
                .map(|c| format!(
                    " Then pull with `{c}` (with pull.ff=only a plain `git pull` refuses)."
                ))
                .unwrap_or_default(),
        ),
    };
    Ok(match settled {
        Settled::Moved(_) => format!("{done} {sentence}"),
        _ => format!(
            "{done} {sentence} If more memory is shared before you pull, run am refresh first."
        ),
    })
}

/// Append `note` on its own line, if any.
fn push_note(out: &mut String, note: &str) {
    if !note.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(note);
    }
}

fn conflict_help(conflicts: &[String]) -> String {
    format!(
        "Your memory and the shared memory both changed: {}. Edit the marked sections (<<<<<<< yours … >>>>>>> shared), then run am share again.",
        conflicts.join(", ")
    )
}

fn kept_note(kept: &[String]) -> String {
    format!(
        "MEMORY.md parts changed both here and in the shared memory; yours were kept: {}",
        kept.join(", ")
    )
}

/// Lock the bank for a local merge, creating its directory if needed.
fn lock_bank(repo: &Repo) -> Result<crate::util::Lock> {
    repo.refuse_links(&repo.bank_rel)?;
    io_at(std::fs::create_dir_all(&repo.bank), &repo.bank)?;
    lock(&repo.bank)
}

/// `am refresh`.
pub fn refresh() -> Result<String> {
    let repo = Repo::current()?;
    let shared = observe(&repo)?;
    let branch = &shared.branch;
    let Some(theirs) = shared.theirs.as_deref() else {
        return Ok(format!(
            "origin/{branch} does not exist yet; nothing to refresh."
        ));
    };
    if shared.base.as_deref() == Some(theirs) {
        let mut out = format!("Memory is up to date with origin/{branch}.");
        // Memory staged earlier (by v0.0.15, say) may still be settled.
        if shared.head_differs && index_holds(&repo, theirs)? {
            let _lock = lock_bank(&repo)?;
            push_note(&mut out, &settle_note(&repo, branch, theirs, None)?);
        }
        return Ok(out);
    }
    let _lock = lock_bank(&repo)?;
    let known: Vec<&str> = shared.base.as_deref().into_iter().chain([theirs]).collect();
    check_staged(&repo, "refresh", &known)?;
    let merged = merge_into_worktree(&repo, theirs, shared.base.as_deref())?;
    record_shared(&repo, theirs)?;
    let mut out = if merged.updated.is_empty() {
        format!("Memory is up to date with origin/{branch}.")
    } else {
        format!(
            "Brought {} memory file(s) from origin/{branch}:\n  {}",
            merged.updated.len(),
            merged.updated.join("\n  ")
        )
    };
    push_note(
        &mut out,
        &settle_note(&repo, branch, theirs, shared.base.as_deref())?,
    );
    if !merged.kept_rows.is_empty() {
        out.push('\n');
        out.push_str(&kept_note(&merged.kept_rows));
    }
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
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file()
                && std::fs::read_to_string(&path)
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
    // After a rejected push, the shared commit just merged is the base.
    let mut merged_base: Option<String> = None;
    for _attempt in 0..3 {
        let shared = observe(&repo)?;
        let branch = &shared.branch;
        let theirs = shared.theirs.as_deref();
        let base = merged_base.clone().or(shared.base.clone());
        let lock = lock_bank(&repo)?;
        // What the index may hold: the recorded (staged) commit, the base, theirs.
        let known: Vec<&str> = [shared.base.as_deref(), base.as_deref(), theirs]
            .into_iter()
            .flatten()
            .collect();
        check_staged(&repo, "share", &known)?;
        let mut merged = Merged::default();
        if let Some(theirs) = theirs.filter(|t| base.as_deref() != Some(*t)) {
            merged = merge_into_worktree(&repo, theirs, base.as_deref())?;
            if !merged.conflicts.is_empty() {
                record_shared(&repo, theirs)?;
                bail!("{}", conflict_help(&merged.conflicts));
            }
            merged_base = Some(theirs.to_owned());
        }
        let snapshot = commit_bank(&repo, theirs, message)?;
        drop(lock);
        let mut notes = String::new();
        if !merged.kept_rows.is_empty() {
            push_note(&mut notes, &kept_note(&merged.kept_rows));
        }
        let Some(commit) = snapshot else {
            let mut out = format!("Nothing new to share; origin/{branch} has all of this memory.");
            if let Some(theirs) = theirs {
                if shared.base.as_deref() != Some(theirs) {
                    let note = stage_shared(&repo, branch, theirs, &known, shared.base.as_deref())?;
                    push_note(&mut out, &note);
                } else if shared.head_differs && index_holds(&repo, theirs)? {
                    let _lock = lock_bank(&repo)?;
                    push_note(&mut out, &settle_note(&repo, branch, theirs, None)?);
                }
            }
            push_note(&mut out, &notes);
            return Ok(out);
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
            let mut out = format!("Shared memory to origin/{branch} (commit {short}).");
            let note = stage_shared(&repo, branch, &commit, &known, shared.base.as_deref())?;
            push_note(&mut out, &note);
            push_note(&mut out, &notes);
            return Ok(out);
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
            return propose(&repo, branch, &commit);
        }
        bail!("Push to origin/{branch} failed: {}", error.trim());
    }
    bail!("The shared branch kept moving while sharing; run am share again.")
}

/// Run `work` with a `git` that uses a private temporary index.
fn temp_index<T>(
    repo: &Repo,
    work: impl FnOnce(&dyn Fn(&[&str]) -> Result<String>) -> Result<T>,
) -> Result<T> {
    let index = std::env::temp_dir().join(format!("am-index-{}", uuid4()));
    let env = [("GIT_INDEX_FILE", index.as_os_str())];
    let git = |args: &[&str]| -> Result<String> {
        let output = repo.git_with(args, &env, None)?;
        if !output.ok() {
            bail!(
                "git failed while preparing the memory commit: {}",
                output.error_text()
            );
        }
        Ok(output.text())
    };
    let result = work(&git);
    let _ = std::fs::remove_file(&index);
    result
}

/// A commit holding the shared tree with this worktree's bank, or `None`
/// when that equals the shared tree.
fn commit_bank(repo: &Repo, theirs: Option<&str>, message: Option<&str>) -> Result<Option<String>> {
    temp_index(repo, |git| {
        match theirs {
            Some(theirs) => git(&["read-tree", theirs])?,
            None => git(&["read-tree", "--empty"])?,
        };
        git(&["add", "-A", "--", &repo.bank_rel])?;
        let changed = match theirs {
            Some(theirs) => git(&["diff", "--cached", "--name-only", "--no-renames", theirs])?,
            None => git(&["ls-files"])?,
        };
        let count = changed.lines().count();
        if count == 0 {
            return Ok(None);
        }
        let tree = git(&["write-tree"])?;
        let subject = message
            .map(str::to_owned)
            .unwrap_or_else(|| format!("memory: update {count} file(s) in {}", repo.bank_rel));
        let mut args = vec!["commit-tree", tree.as_str(), "-m", subject.as_str()];
        if let Some(theirs) = theirs {
            args.extend(["-p", theirs]);
        }
        git(&args).map(Some)
    })
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
