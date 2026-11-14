mod support;
use agent_memory::codec::json::Value;
use agent_memory::console::{Channel as Stream, Transcript};
use support::World;
use support::distribution::Channel;
use support::lifecycle::*;
fn rendered(value: &Value, command: &str) -> String {
    let mut output = Transcript::default();
    agent_memory::console::render(value, command, "am", 0, &mut output);
    output.text(Stream::Stdout)
}

// obligation: tests/contracts/test_public_cli.py::PublicCLIContract.test_human_recovery_after_partial_activation_and_no_pending_transaction
#[test]

fn human_recovery_after_partial_activation_and_no_pending_transaction() {
    let world = World::new().unwrap();
    let source = source(&world);
    let result = fault(
        &world,
        &[
            "install",
            "--development-source",
            source.to_str().unwrap(),
            "--presets",
            "none",
        ],
        "install-after-activate-1",
        "exit",
    );
    assert_eq!(result.status, agent_memory::process::Status::Code(86));
    let cli = world.paths["home"].join(".local/bin/am");
    let output = world.run([cli.to_str().unwrap(), "recover"]).unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("Rolled back interrupted runtime"));
    assert!(!cli.exists());
    let world = World::new().unwrap();
    let result = world.am(["recover"]).unwrap();
    assert_eq!(result.stdout, b"No pending runtime transaction.\n");
}

// obligation: tests/contracts/test_public_cli.py::PublicCLIContract.test_human_bootstrap_preview_repeat_update_and_uninstall_journey_under_temp_alias
#[test]
#[ignore = "needs recovery_frontier registered"]
fn human_signed_lifecycle_journey_under_native_temp_alias() {
    let mut world = World::new().unwrap();
    #[cfg(target_os = "macos")]
    {
        let tmp = world.paths["tmp"]
            .to_string_lossy()
            .replacen("/private/tmp/", "/tmp/", 1);
        world.env.insert("TMPDIR".into(), tmp.into());
    }
    let channel = Channel::new(&world);
    channel.publish(env!("CARGO_PKG_VERSION"), &"a".repeat(40));
    let before = snapshot(&world.root);
    let planned = channel.run(&world, "install", &["none", "copy", "--check"], 0);
    let text = rendered(&planned, "install");
    assert!(text.contains("Installation preview; no changes made"));
    assert!(!text.contains("\"verified\""));
    assert_eq!(snapshot(&world.root), before);
    let first = channel.install(&world, "none", "copy");
    assert!(
        rendered(&first, "install")
            .contains(&format!("Installed am {}", env!("CARGO_PKG_VERSION")))
    );
    let receipt_path = state(&world).join("receipt.json");
    let receipt = get(&first, "receipt");
    let cli = command(receipt);
    let old = std::fs::read(&receipt_path).unwrap();
    let checked = channel.run(&world, "update", &["--check"], 0);
    let text = rendered(&checked, "update");
    assert!(text.contains("is current"));
    assert!(text.contains("Preview only"));
    assert_eq!(std::fs::read(&receipt_path).unwrap(), old);
    std::fs::write(
        channel.pack.join("templates/public-update.txt"),
        b"Updated artifact.\n",
    )
    .unwrap();
    mode(&channel.pack.join("templates/public-update.txt"), 0o644);
    channel.publish("0.0.16", &"b".repeat(40));
    let updated = channel.run(&world, "update", &[], 0);
    assert!(rendered(&updated, "update").contains("Updated am"));
    assert_eq!(
        support::lifecycle::text(get(get(&updated, "receipt"), "source"), "revision"),
        "b".repeat(40)
    );
    let repeated = channel.run(&world, "update", &[], 0);
    assert!(rendered(&repeated, "update").contains("is up to date"));
    let bank = world.paths["repo"].join("memory-bank");
    std::fs::create_dir(&bank).unwrap();
    std::fs::write(bank.join("keep.md"), b"Keep memory.\n").unwrap();
    let preview = world
        .run([cli.to_str().unwrap(), "uninstall", "--check"])
        .unwrap();
    assert!(String::from_utf8_lossy(&preview.stdout).contains("Uninstall preview"));
    assert!(cli.exists());
    let removed = channel.run(&world, "uninstall", &[], 0);
    let text = rendered(&removed, "uninstall");
    assert!(text.contains("Uninstalled owned runtime"));
    assert!(text.contains("memory and user data preserved"));
    assert!(!cli.exists());
    assert_eq!(
        std::fs::read(bank.join("keep.md")).unwrap(),
        b"Keep memory.\n"
    );
}
