#![allow(dead_code)]

pub mod identity_fixtures;
pub mod record_fixtures;
pub mod world;

#[allow(unused_imports)]
pub use world::{
    Child, Command, Environment, Result, RunOptions, World, WorldError, require_current_world,
    require_world,
};

/// Run a direct-domain consumer against the real frontier in a private World.
/// This injects test runtime locations only; native am children still use receipts.
pub fn with_frontier(name: &str, body: impl FnOnce()) {
    use agent_memory::error::ContractError;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::Arc;

    const CASE: &str = "AM_FRONTIER_CONSUMER_CASE";
    const COMPLETED: &str = "frontier-consumer-completed";
    if std::env::var(CASE).as_deref() == Ok(name) {
        let root = require_current_world().unwrap();
        agent_memory::cli::register_providers().unwrap();
        let frontier = Arc::new(agent_memory::recovery_frontier::FrontierProvider::new(
            || {
                let root = require_current_world()
                    .map_err(|error| ContractError::new(error.to_string()))?;
                Ok((
                    agent_memory::fs::canonical_path(&root.join("data"), true)?,
                    std::env::current_exe()
                        .map_err(|error| ContractError::new(error.to_string()))?,
                ))
            },
        ));
        agent_memory::bank_writes::register_recovery_frontier(Some(frontier.clone()));
        agent_memory::bank_contract::register_recovery_coverage(Some(frontier));
        body();
        // No receipt is created if dispatch skipped the test or an assertion failed.
        let token = std::env::var("AGENT_MEMORY_TEST_TOKEN").unwrap();
        let mut receipt = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.join(COMPLETED))
            .unwrap();
        receipt
            .write_all(format!("{name}\n{token}\n").as_bytes())
            .unwrap();
        receipt.sync_all().unwrap();
        std::fs::File::open(&root).unwrap().sync_all().unwrap();
        return;
    }
    let mut world = World::new().unwrap();
    let receipt = world.root.join(COMPLETED);
    let expected = format!("{name}\n{}\n", world.token);
    assert!(!receipt.exists());
    world.env.insert(CASE.into(), name.into());
    world
        .run([
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            name.into(),
            "--include-ignored".into(),
            "--nocapture".into(),
        ])
        .unwrap();
    assert_eq!(std::fs::read(receipt).unwrap(), expected.as_bytes());
}
