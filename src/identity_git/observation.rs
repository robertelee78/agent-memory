//! A real Git batch supplies configuration and checkout facts for one observation.
//! Arbitrary PATH wrappers keep the ordinary query path, including fault shims.
use super::*;
use std::os::unix::fs::PermissionsExt;

struct ConfigEntry {
    local: bool,
    key: Vec<u8>,
    value: Vec<u8>,
}
pub(super) struct Facts {
    probes: Vec<(Invocation, Output)>,
    config: Vec<ConfigEntry>,
    root: PathBuf,
    common: OsString,
    head: String,
    remote: Output,
}
fn stock_git() -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let selected = std::env::split_paths(&path)
        .map(|path| path.join("git"))
        .find(|path| {
            std::fs::metadata(path)
                .is_ok_and(|info| info.is_file() && info.permissions().mode() & 0o111 != 0)
        });
    selected.and_then(|path| std::fs::canonicalize(path).ok())
        == std::fs::canonicalize("/usr/bin/git").ok()
}
fn launch(invocations: &[Invocation]) -> Option<Vec<Output>> {
    let specs: Vec<_> = invocations
        .iter()
        .map(|invocation| spec(invocation, Deadline::from_absolute_ns(u64::MAX)))
        .collect();
    let mut group = process::Group::launch(&specs, Duration::from_secs(10)).ok()?;
    let result = (0..invocations.len())
        .map(|index| {
            let output = group.value(index).ok()?;
            #[cfg(feature = "dev-tools")]
            let _ = crate::runtime_faults::git_spawn(output.pid);
            (output.status == Status::Code(0)).then_some(output)
        })
        .collect::<Option<Vec<_>>>();
    group.finish().ok()?;
    result
}
impl Facts {
    pub(super) fn capture(root: &Path) -> Option<Self> {
        if !stock_git() {
            return None;
        }
        #[cfg(feature = "dev-tools")]
        if std::env::var_os("AGENT_MEMORY_FAULT_STAGE").is_some() {
            return None;
        }
        let commands: &[(&[&str], bool)] = &[
            (
                &[
                    "config",
                    "--null",
                    "--show-origin",
                    "--show-scope",
                    "--list",
                ],
                false,
            ),
            (
                &[
                    "rev-parse",
                    "--show-toplevel",
                    "--git-common-dir",
                    "--verify",
                    "--quiet",
                    "HEAD",
                ],
                false,
            ),
            (
                &[
                    "ls-files",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "-t",
                    "-z",
                    "--",
                    ":(icase,glob)**/MEMORY.md",
                ],
                true,
            ),
        ];
        let invocations: Vec<_> = commands
            .iter()
            .map(|(args, glob)| {
                _invocation(
                    root,
                    &args.iter().map(OsString::from).collect::<Vec<_>>(),
                    *glob,
                )
            })
            .collect();
        let outputs = launch(&invocations)?;
        let raw = &outputs[1].stdout;
        let without_lf = raw.strip_suffix(b"\n")?;
        let split = without_lf.iter().rposition(|byte| *byte == b'\n')?;
        let head = std::str::from_utf8(&without_lf[split + 1..]).ok()?;
        if !matches!(head.len(), 40 | 64)
            || !head
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        let prefix = [root.as_os_str().as_bytes(), b"\n"].concat();
        let common = without_lf[..split].strip_prefix(prefix.as_slice())?;
        let common = OsString::from_vec(common.to_vec());
        let head = head.to_owned();
        let mut fields: Vec<_> = outputs[0].stdout.split(|byte| *byte == 0).collect();
        if fields.pop()? != b"" || fields.len() % 3 != 0 {
            return None;
        }
        let mut config = Vec::with_capacity(fields.len() / 3);
        for entry in fields.chunks_exact(3) {
            let split = entry[2].iter().position(|byte| *byte == b'\n')?;
            config.push(ConfigEntry {
                local: entry[0] == b"local",
                key: entry[2][..split].to_vec(),
                value: entry[2][split + 1..].to_vec(),
            });
        }
        let remote_invocation = _invocation(root, &[OsString::from("remote")], false);
        let tree_invocation = _invocation(
            root,
            &["ls-tree", "-r", "--name-only", "-z", &head].map(OsString::from),
            false,
        );
        let remaining = vec![remote_invocation, tree_invocation];
        let outputs_remaining = launch(&remaining)?;
        let remote = copy_output(&outputs_remaining[0]);
        let mut probes: Vec<_> = invocations.into_iter().zip(outputs).collect();
        probes.extend(remaining.into_iter().zip(outputs_remaining));
        Some(Self {
            probes,
            config,
            root: root.to_path_buf(),
            common,
            head,
            remote,
        })
    }
    pub(super) fn output(&self, invocation: &Invocation) -> Option<Output> {
        self.probes
            .iter()
            .find(|(key, _)| key == invocation)
            .map(|(_, output)| copy_output(output))
    }
    pub(super) fn root_head(&self) -> (Option<OsString>, Option<String>) {
        (
            Some(self.root.as_os_str().to_owned()),
            Some(self.head.clone()),
        )
    }
    pub(super) fn value(&self, invocation: &Invocation) -> Option<Option<OsString>> {
        if invocation.env != self.probes[0].0.env
            || invocation.argv.get(9).map(OsString::as_os_str) != Some(self.root.as_os_str())
        {
            return None;
        }
        let args: Vec<_> = invocation.argv[10..]
            .iter()
            .map(OsString::as_os_str)
            .collect();
        let equals = |expected: &[&str]| {
            args.iter()
                .map(|arg| arg.as_bytes())
                .eq(expected.iter().map(|arg| arg.as_bytes()))
        };
        if equals(&["rev-parse", "--show-toplevel"]) {
            return Some(Some(self.root.as_os_str().to_owned()));
        }
        if equals(&["rev-parse", "--git-common-dir"]) {
            return Some(Some(self.common.clone()));
        }
        if equals(&["rev-parse", "--verify", "HEAD"]) {
            return Some(Some(OsString::from(&self.head)));
        }
        if equals(&["remote"]) {
            let mut raw = self.remote.stdout.clone();
            if raw.last() == Some(&b'\n') {
                raw.pop();
            }
            return Some(Some(OsString::from_vec(raw)));
        }
        let (local, null, regexp, key) = if equals(&[
            "config",
            "--local",
            "--null",
            "--get-regexp",
            "^agent-memory\\.(bankroot|authoritybranch)$",
        ]) {
            (true, true, true, b"bank".as_slice())
        } else if equals(&["config", "--null", "--get-regexp", "^user\\.(email|name)$"]) {
            (false, true, true, b"user".as_slice())
        } else if equals(&["config", "--get-all", "agent-memory.authorityRemote"]) {
            (
                false,
                false,
                false,
                b"agent-memory.authorityremote".as_slice(),
            )
        } else if args.len() == 5
            && args[0] == "config"
            && args[1] == "--local"
            && args[2] == "--null"
            && args[3] == "--get-all"
            && matches!(
                args[4].to_str(),
                Some("agent-memory.bankRoot" | "agent-memory.authorityBranch")
            )
        {
            (true, true, false, args[4].as_bytes())
        } else {
            return None;
        };
        let key = key.to_ascii_lowercase();
        let selected: Vec<_> = self
            .config
            .iter()
            .filter(|entry| {
                (!local || entry.local)
                    && if regexp {
                        if key == b"bank" {
                            matches!(
                                entry.key.as_slice(),
                                b"agent-memory.bankroot" | b"agent-memory.authoritybranch"
                            )
                        } else {
                            matches!(entry.key.as_slice(), b"user.name" | b"user.email")
                        }
                    } else {
                        entry.key == key
                    }
            })
            .collect();
        if selected.is_empty() {
            return Some(None);
        }
        let mut raw = Vec::new();
        for (index, entry) in selected.iter().enumerate() {
            if !null && index != 0 {
                raw.push(b'\n');
            }
            if regexp {
                raw.extend_from_slice(&entry.key);
                raw.push(b'\n');
            }
            raw.extend_from_slice(&entry.value);
            if null {
                raw.push(0);
            }
        }
        Some(Some(OsString::from_vec(raw)))
    }
    pub(super) fn recheck(&self) -> crate::error::Result<()> {
        for chunk in self.probes.chunks(3) {
            let specs: Vec<_> = chunk
                .iter()
                .map(|(invocation, _)| spec(invocation, Deadline::from_absolute_ns(u64::MAX)))
                .collect();
            let mut group = process::Group::launch(&specs, Duration::from_secs(10))
                .map_err(|error| ContractError::new(error.to_string()))?;
            let checked = (|| {
                for (index, (_, expected)) in chunk.iter().enumerate() {
                    let actual = group
                        .value(index)
                        .map_err(|error| ContractError::new(error.to_string()))?;
                    #[cfg(feature = "dev-tools")]
                    let _ = crate::runtime_faults::git_spawn(actual.pid);
                    if actual.status != expected.status || actual.stdout != expected.stdout {
                        return Err(ContractError::identity_unresolved(
                            "Repository identity evidence changed during observation",
                        ));
                    }
                }
                Ok(())
            })();
            let finished = group
                .finish()
                .map_err(|error| ContractError::new(error.to_string()));
            checked?;
            finished?;
        }
        Ok(())
    }
}
