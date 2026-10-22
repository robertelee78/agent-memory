"""File-only checkpoint/resume and subsequent real observed-write validation."""
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
WRITES = ROOT / "scripts/bank_writes.py"
RENDER = ROOT / "scripts/bank_contract.py"
CONTEXTS = ["projectbrief.md", "productContext.md", "activeContext.md", "systemPatterns.md", "techContext.md", "progress.md"]
BODY = """# Active context

## Current focus
Verify the component-specific retry lesson; [task](../task.md).

## Recent changes
The branch's isolated test passed; [evidence](../test-result.md).

## Open questions
Applicability outside this component remains unverified.

## Next step
Inspect the first clone's result.
"""


class ManualCheckpointContract(unittest.TestCase):
    def command(self, world, *args):
        result = world.run([sys.executable, "-I", WRITES, *args], check=False)
        return result, json.loads(result.stdout)

    def bank(self, world):
        repo = world.repository()
        proposal = {"project-id": str(uuid.uuid4()), "router-updated": "2026-10-22T18:00:00Z",
                    "contexts": {"activeContext.md": {"body": BODY, "purpose": "Current focus and next step",
                                 "scope": "component:retry", "lifecycle": "active", "read-when": "When resuming retry work"}},
                    "omitted": {name: "No additional evidence supplied" for name in CONTEXTS if name != "activeContext.md"}}
        file = world.inside("router.json")
        file.write_text(json.dumps(proposal))
        result = world.run([sys.executable, "-I", RENDER, "render", "--input", file])
        bank = repo / "memory-bank"
        bank.mkdir()
        for name, content in json.loads(result.stdout)["files"].items():
            (bank / name).write_text(content)
        world.git("add", ".", cwd=repo)
        world.git("commit", "-m", "Fixture checkpoint base", cwd=repo)
        return repo, bank

    def preview(self, world, bank, body):
        file = world.inside("reviewed-context.md")
        file.write_text(body)
        return self.command(world, "preview", "--repo", bank.parent, "--path", "activeContext.md", "--content", file)

    def apply(self, world, proposal):
        file = world.inside("write-proposal.json")
        file.write_text(json.dumps(proposal))
        return self.command(world, "apply", "--proposal", file)

    def test_file_only_checkpoint_resumes_without_generated_state_and_preserves_other_work(self):
        with World() as world:
            repo, bank = self.bank(world)
            (repo / "feature.py").write_text("unfinished = True\n")
            world.git("add", "feature.py", cwd=repo)
            (repo / "scratch.txt").write_text("Unrelated unstaged work\n")
            index = (repo / ".git/index").read_bytes()
            head = world.git("rev-parse", "HEAD", cwd=repo).stdout
            router = (bank / "MEMORY.md").read_bytes()
            # No installed CLI, registry, generated state or cache is used below.
            for name in ("config", "state", "cache"):
                shutil.rmtree(world.paths[name])
            target = bank / "activeContext.md"
            self.assertFalse((bank / ".agent-memory.lock").exists())
            self.assertFalse((bank / ".agent-memory-transaction.md").exists())
            before = target.read_text()
            self.assertEqual(before, BODY)
            updated = before.replace("Inspect the first clone's result.", "Inspect the second clone's refresh result before broadening applicability.")
            target.write_text(updated)
            self.assertEqual(target.read_text(), updated)
            self.assertEqual(world.git("diff", "--name-only", cwd=repo).stdout.strip(), "memory-bank/activeContext.md")
            self.assertEqual((repo / ".git/index").read_bytes(), index)
            self.assertEqual(world.git("rev-parse", "HEAD", cwd=repo).stdout, head)
            self.assertEqual((repo / "scratch.txt").read_text(), "Unrelated unstaged work\n")
            self.assertEqual((bank / "MEMORY.md").read_bytes(), router)
            resumed = world.run(["cat", target]).stdout
            for retained in ("## Current focus", "[task](../task.md)", "[evidence](../test-result.md)",
                             "remains unverified", "second clone's refresh result"):
                self.assertIn(retained, resumed)
            self.assertFalse(world.paths["state"].exists())
            # Ordinary Git recovers this exact file without resetting unrelated work.
            base = world.git("show", "HEAD:memory-bank/activeContext.md", cwd=repo).stdout
            target.write_text(base)
            self.assertEqual(target.read_text(), BODY)
            self.assertEqual((repo / ".git/index").read_bytes(), index)
            self.assertEqual((repo / "scratch.txt").read_text(), "Unrelated unstaged work\n")

    def test_stale_mediated_proposal_refuses_then_fresh_write_preserves_manual_evidence(self):
        with World() as world:
            repo, bank = self.bank(world)
            result, old = self.preview(world, bank, BODY.replace("first clone", "old in-session snapshot"))
            self.assertEqual(result.returncode, 0, old)
            target = bank / "activeContext.md"
            current = target.read_text().replace("Inspect the first clone's result.", "Inspect the second clone's result; [manual evidence](../manual-result.md).")
            target.write_text(current)
            result, refused = self.apply(world, old)
            self.assertEqual(result.returncode, 1, refused)
            self.assertFalse(refused["verified"])
            self.assertEqual(target.read_text(), current)
            reread = target.read_text()
            reviewed = reread.replace("Verify the component-specific retry lesson", "Continue checking the component-specific retry lesson")
            result, fresh = self.preview(world, bank, reviewed)
            self.assertEqual(result.returncode, 0, fresh)
            self.assertNotEqual(old["proposal"]["observed_hashes"], fresh["proposal"]["observed_hashes"])
            result, applied = self.apply(world, fresh)
            self.assertEqual(result.returncode, 0, applied)
            self.assertTrue(applied["verified"])
            self.assertEqual(target.read_text(), reviewed)
            self.assertIn("[manual evidence](../manual-result.md)", target.read_text())
            self.assertIn("[evidence](../test-result.md)", target.read_text())
            self.assertIn("remains unverified", target.read_text())
            self.assertNotIn("old in-session snapshot", target.read_text())

    def test_visible_lock_and_interrupted_bank_stop_mediated_resume_without_repair(self):
        with World() as world:
            repo, bank = self.bank(world)
            original = (bank / "activeContext.md").read_bytes()
            for name in (".agent-memory.lock", ".agent-memory-transaction.md"):
                marker = bank / name
                marker.write_text("Unrecognized owner/state requires inspection; do not remove by age.\n")
                result, receipt = self.command(world, "inspect", "--repo", repo)
                self.assertEqual(result.returncode, 1, receipt)
                result, receipt = self.preview(world, bank, BODY + "\nDo not apply during interruption.\n")
                self.assertEqual(result.returncode, 1, receipt)
                self.assertEqual((bank / "activeContext.md").read_bytes(), original)
                self.assertTrue(marker.exists())
                marker.unlink()  # Fixture owner removes its own marker, not age-based product recovery.

    def test_invalid_external_edit_is_reported_instead_of_template_repair(self):
        with World() as world:
            repo, bank = self.bank(world)
            result, old = self.preview(world, bank, BODY)
            self.assertEqual(result.returncode, 0, old)
            target = bank / "activeContext.md"
            target.write_text("   \n")
            result, refused = self.apply(world, old)
            self.assertEqual(result.returncode, 1, refused)
            self.assertFalse(refused["verified"])
            self.assertEqual(target.read_text(), "   \n")
            result, fresh = self.preview(world, bank, BODY)
            self.assertEqual(result.returncode, 1, fresh)
            self.assertEqual(target.read_text(), "   \n")


if __name__ == "__main__":
    unittest.main()
