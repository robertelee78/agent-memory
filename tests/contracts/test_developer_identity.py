"""Actual initializer/selection commands; no installed CLI or project bank."""
import json
import os
from pathlib import Path
import shutil
import socket
import stat
import sys
import time
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/developer_identity.py"
PACK = ROOT / "skills/agent-memory"


class DeveloperIdentityContract(unittest.TestCase):
    def command(self, world, *args, helper=HELPER):
        result = world.run([sys.executable, "-I", helper, *args], check=False)
        return result, json.loads(result.stdout)

    def preview(self, world, *args, helper=HELPER):
        result, receipt = self.command(world, "preview", *args, helper=helper)
        self.assertEqual(result.returncode, 0, receipt)
        return receipt

    def apply(self, world, proposal, helper=HELPER):
        path = world.inside("proposal.json")
        path.write_text(json.dumps(proposal))
        return self.command(world, "apply", "--proposal", path, helper=helper)

    def create(self, world, *args):
        proposal = self.preview(world, "--create", *args)
        result, receipt = self.apply(world, proposal)
        self.assertEqual(result.returncode, 0, receipt)
        self.assertTrue(receipt["verified"])
        return receipt["developer_id"], proposal

    def registry(self, world):
        return Path(world.env["AGENT_MEMORY_CONFIG_HOME"]) / "developers.json"

    def lock(self, world):
        return self.registry(world).parent / ".developers.lock"

    def stale_owner(self, world):
        return int(world.run([sys.executable, "-c", "import os; print(os.getpid())"]).stdout)

    def lock_data(self, pid):
        return {"host": socket.gethostname(), "pid": pid, "operation": "developer-create",
                "operation_id": str(uuid.uuid4()), "start": "2026-10-22T17:00:00Z"}

    def test_first_setup_has_no_registry_bank_install_or_credentials_prerequisite(self):
        with World() as world:
            result, status = self.command(world, "status")
            self.assertEqual(result.returncode, 0)
            self.assertTrue(status["setup_required"])
            self.assertFalse(status["identity_authenticated"])
            self.assertIn("not authentication", status["security_boundary"])
            self.assertFalse(self.registry(world).parent.exists())
            proposal = self.preview(world, "--create", "--label", "Developer A")
            self.assertFalse(self.registry(world).parent.exists())
            self.assertFalse(proposal["writes_performed"])
            result, receipt = self.apply(world, proposal)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["verified"])
            self.assertEqual(receipt["changed_memory_paths"], [])
            self.assertEqual(stat.S_IMODE(self.registry(world).stat().st_mode), 0o600)
            self.assertFalse((world.paths["repo"] / "memory-bank").exists())
            self.assertFalse((world.paths["repo"] / ".git").exists())

    def test_same_id_is_explicitly_adopted_across_five_machines(self):
        shared = None
        for number in range(5):
            with World() as world:
                if shared is None:
                    shared, _ = self.create(world, "--label", "Developer A")
                else:
                    proposal = self.preview(world, "--adopt", shared, "--label", "Developer A")
                    self.assertEqual(self.apply(world, proposal)[0].returncode, 0)
                result, status = self.command(world, "status", "--harness", ["claude", "codex", "opencode", "custom", "codex"][number],
                                              "--session", f"session-{number}")
                self.assertEqual(result.returncode, 0, status)
                self.assertEqual(status["developer_id"], shared)
                registry = json.loads(self.registry(world).read_text())
                self.assertEqual(set(registry["developers"][shared]), {"labels", "aliases"})
                self.assertNotIn("session", self.registry(world).read_text())
                self.assertNotIn("harness", self.registry(world).read_text())

    def test_collaborator_selection_is_explicit_and_preserves_metadata(self):
        with World() as world:
            original, _ = self.create(world, "--label", "Developer A", "--alias", "github-login=alice")
            other, _ = self.create(world, "--label", "Developer B", "--alias", "github-login=bob")
            self.assertNotEqual(original, other)
            proposal = self.preview(world, "--select", original)
            self.assertEqual(self.apply(world, proposal)[1]["developer_id"], original)
            proposal = self.preview(world, "--adopt", original, "--label", "Same person on this machine", "--alias", "os-user=local-alias")
            self.assertEqual(self.apply(world, proposal)[0].returncode, 0)
            data = json.loads(self.registry(world).read_text())
            self.assertEqual(len(data["developers"]), 2)
            self.assertEqual(data["developers"][original]["labels"], ["Developer A", "Same person on this machine"])
            self.assertIn({"kind": "github-login", "value": "alice"}, data["developers"][original]["aliases"])
            self.assertEqual(data["developers"][other]["labels"], ["Developer B"])

    def test_changed_git_hosting_client_and_user_aliases_never_change_attribution(self):
        with World() as world:
            repo = world.repository()
            world.git("config", "user.name", "Developer A", cwd=repo)
            world.git("config", "user.email", "alice@example.invalid", cwd=repo)
            original, _ = self.create(world, "--alias", "git-author-email=alice@example.invalid", "--alias", "github-login=alice")
            before = self.registry(world).read_bytes()
            _, status = self.command(world, "status", "--observe", "github-login=alice", "--harness", "claude", "--session", "one")
            self.assertEqual(status["developer_id"], original)
            world.git("config", "user.email", "bob@example.invalid", cwd=repo)
            world.git("config", "user.name", "Developer B", cwd=repo)
            _, status = self.command(world, "status", "--observe", "github-login=bob", "--observe", "os-user=another-user",
                                     "--harness", "custom", "--session", "two")
            self.assertEqual(status["developer_id"], original)
            self.assertTrue(any(item["state"] == "unknown" for item in status["observations"]))
            self.assertEqual(self.registry(world).read_bytes(), before)
            other, _ = self.create(world, "--alias", "git-author-email=bob@example.invalid")
            proposal = self.preview(world, "--select", original)
            self.assertEqual(self.apply(world, proposal)[0].returncode, 0)
            before = self.registry(world).read_bytes()
            _, status = self.command(world, "status")
            self.assertEqual(status["developer_id"], original)
            self.assertIn(other, status["attribution_conflicts"][0]["matching_developer_ids"])
            self.assertFalse(status["identity_authenticated"])
            self.assertEqual(self.registry(world).read_bytes(), before)

    def test_replay_is_idempotent_without_reselecting_an_old_identity(self):
        with World() as world:
            first, proposal = self.create(world)
            before = self.registry(world).read_bytes()
            result, replay = self.apply(world, proposal)
            self.assertEqual(result.returncode, 0, replay)
            self.assertTrue(replay["already_applied"])
            self.assertTrue(replay["verified"])
            self.assertEqual(self.registry(world).read_bytes(), before)
            second, _ = self.create(world)
            result, replay = self.apply(world, proposal)
            self.assertEqual(result.returncode, 0, replay)
            self.assertEqual(replay["developer_id"], second)
            self.assertEqual(replay["operation_developer_id"], first)
            self.assertEqual(len(json.loads(self.registry(world).read_text())["operations"]), 2)
            forged = json.loads(json.dumps(proposal))
            forged["proposal"]["labels"] = ["Changed request"]
            self.assertEqual(self.apply(world, forged)[0].returncode, 1)

    def test_stale_preview_and_external_edits_stop_without_overwrite(self):
        with World() as world:
            a = self.preview(world, "--create")
            b = self.preview(world, "--create")
            self.assertEqual(self.apply(world, a)[0].returncode, 0)
            before = self.registry(world).read_bytes()
            result, receipt = self.apply(world, b)
            self.assertEqual(result.returncode, 1, receipt)
            self.assertIn("changed after preview", receipt["error"])
            self.assertEqual(self.registry(world).read_bytes(), before)
            selected = json.loads(before)["selected"]
            selection = self.preview(world, "--select", selected)
            data = json.loads(before)
            data["developers"][selected]["labels"].append("External metadata edit")
            self.registry(world).write_text(json.dumps(data))
            before = self.registry(world).read_bytes()
            self.assertEqual(self.apply(world, selection)[0].returncode, 1)
            self.assertEqual(self.registry(world).read_bytes(), before)

    def test_registry_and_proposal_refuse_preferences_harness_aliases_and_wrong_roots(self):
        with World() as world:
            self.assertEqual(self.command(world, "preview", "--adopt", "not-a-uuid")[0].returncode, 1)
            self.assertEqual(self.command(world, "preview", "--create", "--alias", "harness=codex")[0].returncode, 1)
            proposal = self.preview(world, "--create")
            proposal["proposal"]["registry_path"] = str(world.inside("different-profile/developers.json"))
            self.assertEqual(self.apply(world, proposal)[0].returncode, 1)
            self.assertFalse(self.registry(world).exists())
            identity, _ = self.create(world)
            data = json.loads(self.registry(world).read_text())
            data["developers"][identity]["preferences"] = {"color": "yellow"}
            self.registry(world).write_text(json.dumps(data))
            before = self.registry(world).read_bytes()
            self.assertEqual(self.command(world, "status")[0].returncode, 1)
            self.assertEqual(self.command(world, "preview", "--create")[0].returncode, 1)
            self.assertEqual(self.registry(world).read_bytes(), before)

    def test_true_concurrent_initializers_preserve_the_winner(self):
        with World() as world:
            proposals = [self.preview(world, "--create") for _ in range(2)]
            paths = [world.inside(f"proposal-{i}.json") for i in range(2)]
            for path, proposal in zip(paths, proposals):
                path.write_text(json.dumps(proposal))
            children = [world.spawn([sys.executable, "-I", HELPER, "apply", "--proposal", path]) for path in paths]
            receipts = []
            for child in children:
                out, err = child.communicate(timeout=20)
                self.assertFalse(err)
                receipts.append((child.returncode, json.loads(out)))
            self.assertEqual(sum(code == 0 for code, _ in receipts), 1, receipts)
            data = json.loads(self.registry(world).read_text())
            self.assertEqual(len(data["developers"]), 1)
            self.assertEqual(len(data["operations"]), 1)
            self.assertFalse(self.lock(world).exists())

    def test_unrelated_or_symlinked_registry_is_preserved(self):
        with World() as world:
            proposal = self.preview(world, "--create")
            registry = self.registry(world)
            registry.parent.mkdir()
            registry.write_text("Unrelated user file; preserve it.\n")
            before = registry.read_bytes()
            self.assertEqual(self.apply(world, proposal)[0].returncode, 1)
            self.assertEqual(registry.read_bytes(), before)
            registry.unlink()
            external = world.inside("unrelated.json")
            external.write_text("Unrelated content")
            registry.symlink_to(external)
            self.assertEqual(self.apply(world, proposal)[0].returncode, 1)
            self.assertTrue(registry.is_symlink())
            self.assertEqual(external.read_text(), "Unrelated content")

    def test_live_and_foreign_locks_refuse_and_dead_local_owner_can_recover(self):
        with World() as world:
            self.create(world)
            proposal = self.preview(world, "--create")
            lock = self.lock(world)
            lock.write_text(json.dumps(self.lock_data(os.getpid())))
            self.assertEqual(self.apply(world, proposal)[0].returncode, 1)
            self.assertEqual(self.command(world, "recover")[0].returncode, 1)
            data = self.lock_data(self.stale_owner(world))
            data["host"] = "different-machine"
            lock.write_text(json.dumps(data))
            self.assertEqual(self.command(world, "recover")[0].returncode, 1)
            data["host"] = socket.gethostname()
            lock.write_text(json.dumps(data))
            result, recovery = self.command(world, "recover")
            if os.name != "posix":
                self.assertEqual(result.returncode, 1)
                return
            self.assertEqual(result.returncode, 0, recovery)
            file = world.inside("recovery.json")
            file.write_text(json.dumps(recovery))
            data["operation_id"] = str(uuid.uuid4())
            lock.write_text(json.dumps(data))
            self.assertEqual(self.command(world, "recover", "--proposal", file)[0].returncode, 1)
            self.assertTrue(lock.exists())
            _, recovery = self.command(world, "recover")
            file.write_text(json.dumps(recovery))
            before = self.registry(world).read_bytes()
            result, receipt = self.command(world, "recover", "--proposal", file)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["verified"])
            self.assertFalse(lock.exists())
            self.assertEqual(self.registry(world).read_bytes(), before)

    def test_recovery_cannot_remove_a_lock_held_by_another_recovery(self):
        if os.name != "posix":
            return  # Refusal of unsupported automatic recovery is checked above.
        with World() as world:
            self.create(world)
            lock = self.lock(world)
            lock.write_text(json.dumps(self.lock_data(self.stale_owner(world))))
            ready = world.inside("ready")
            script = "import fcntl,sys,time; from pathlib import Path; f=open(sys.argv[1]); fcntl.flock(f,fcntl.LOCK_EX); Path(sys.argv[2]).touch(); time.sleep(60)"
            child = world.spawn([sys.executable, "-c", script, lock, ready])
            deadline = time.monotonic() + 5
            while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(ready.exists())
            before = lock.read_bytes()
            result, receipt = self.command(world, "recover")
            self.assertEqual(result.returncode, 1, receipt)
            self.assertIn("held", receipt["error"])
            self.assertEqual(lock.read_bytes(), before)
            # World cleanup kills/reaps this still-live concurrent process.

    def test_copied_pack_and_config_paths_with_spaces_work_without_installation(self):
        with World() as world:
            pack = world.inside("portable pack with spaces")
            shutil.copytree(PACK, pack)
            config = world.inside("config with spaces")
            world.env["AGENT_MEMORY_CONFIG_HOME"] = str(config)
            helper = pack / "scripts/developer_identity.py"
            proposal = self.preview(world, "--create", "--label", "Developer A", helper=helper)
            result, receipt = self.apply(world, proposal, helper=helper)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["verified"])
            self.assertEqual(self.command(world, "status", helper=helper)[1]["developer_id"], proposal["developer_id"])


if __name__ == "__main__":
    unittest.main()
