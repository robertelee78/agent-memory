use super::World;
use super::distribution::Channel;
use super::lifecycle::{get, save, text};
use agent_memory::codec::json::{self, Value};
use std::path::PathBuf;
pub struct Canary {
    pub channel: Channel,
    pub repo: PathBuf,
    pub bank: PathBuf,
    pub project: String,
    pub developer: String,
}
impl Canary {
    pub fn new(world: &World) -> Self {
        let channel = Channel::new(world);
        let repo = world.repository("canary project", false).unwrap();
        std::fs::write(repo.join("app.rs"), b"const RETRY_LIMIT: usize = 3;\n").unwrap();
        std::fs::write(
            repo.join("README.md"),
            b"# Retry investigation\n\nReproduce before generalizing.\n",
        )
        .unwrap();
        let project = agent_memory::host::uuid4().unwrap();
        let mut request = json::decode(r##"{"router-updated":"2026-10-22T18:00:00Z","contexts":{"activeContext.md":{"body":"# Focus\n\nInvestigate retry behavior.\n\n## Original evidence\n\nSource: [implementation](../app.rs); applicability elsewhere is unknown.\n","purpose":"Retry investigation","scope":"component:retry","lifecycle":"active","read-when":"When changing retry behavior"},"progress.md":{"body":"# Legacy context\n\nRETIRED_BRANCH_ONLY_CANARY\n","purpose":"Earlier branch history","scope":"branch:legacy","lifecycle":"expired","read-when":"Only when investigating the legacy branch"}},"omitted":{"projectbrief.md":"No distinct evidence","productContext.md":"No distinct evidence","systemPatterns.md":"No distinct evidence","techContext.md":"No distinct evidence"}}"##, Default::default()).unwrap();
        if let Value::Object(fields) = &mut request {
            fields.push(("project-id".into(), Value::String(project.clone())));
        }
        let input = world.root.join("canary-render.json");
        save(&input, &request);
        let rendered = channel.run(
            world,
            "cli",
            &["bank", "render", "--input", input.to_str().unwrap()],
            0,
        );
        let bank = repo.join("memory-bank");
        std::fs::create_dir(&bank).unwrap();
        for (path, body) in get(&rendered, "files").as_object().unwrap() {
            std::fs::write(bank.join(path), body.as_str().unwrap()).unwrap();
        }
        let proposal = channel.run(
            world,
            "cli",
            &[
                "developer",
                "preview",
                "--create",
                "--label",
                "Disposable canary developer",
            ],
            0,
        );
        save(&input, &proposal);
        let applied = channel.run(
            world,
            "cli",
            &["developer", "apply", "--proposal", input.to_str().unwrap()],
            0,
        );
        let developer = text(&applied, "developer_id").to_owned();
        let proposal = channel.run(
            world,
            "cli",
            &[
                "identity",
                "preview",
                "--repo",
                repo.to_str().unwrap(),
                "--action",
                "bind",
                "--reason",
                "Explicit disposable canary bank adoption",
            ],
            0,
        );
        save(&input, &proposal);
        let applied = channel.run(
            world,
            "cli",
            &["identity", "apply", "--proposal", input.to_str().unwrap()],
            0,
        );
        assert_eq!(get(&applied, "verified"), &Value::Bool(true));
        let git = |args: &[&str]| {
            world
                .git_with(
                    args,
                    super::RunOptions {
                        cwd: Some(&repo),
                        ..Default::default()
                    },
                )
                .unwrap()
        };
        git(&["add", "."]);
        git(&["commit", "-m", "Disposable source and memory bank"]);
        std::fs::write(
            repo.join("app.rs"),
            b"const RETRY_LIMIT: usize = 4; // unrelated staged work\n",
        )
        .unwrap();
        git(&["add", "app.rs"]);
        Self {
            channel,
            repo,
            bank,
            project,
            developer,
        }
    }
    pub fn policy(&self, world: &World, changes: &[(&str, Value)], expected: i32) -> Value {
        let mut request = json::decode(r#"{"category":"admission","source_class":"assistant","content_kind":"summary","text":"Retry behavior needs independent reproduction in this component.","record_ids":[],"automatic":true,"repository_evidence":null}"#, Default::default()).unwrap();
        if let Value::Object(fields) = &mut request {
            fields.push(("project_id".into(), Value::String(self.project.clone())));
            for (key, value) in changes {
                if let Some((_, old)) = fields.iter_mut().find(|(name, _)| name == key) {
                    *old = value.clone();
                } else {
                    fields.push(((*key).to_owned(), value.clone()));
                }
            }
        }
        let path = world.root.join("canary-policy.json");
        save(&path, &request);
        self.channel.run(
            world,
            "cli",
            &[
                "security-policy",
                "--repo",
                self.repo.to_str().unwrap(),
                "--request",
                path.to_str().unwrap(),
            ],
            expected,
        )
    }
    pub fn code(&self, world: &World) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        (
            std::fs::read(self.repo.join("app.rs")).unwrap(),
            std::fs::read(self.repo.join(".git/index")).unwrap(),
            world
                .git_with(
                    ["rev-parse", "HEAD"],
                    super::RunOptions {
                        cwd: Some(&self.repo),
                        ..Default::default()
                    },
                )
                .unwrap()
                .stdout,
        )
    }
}
