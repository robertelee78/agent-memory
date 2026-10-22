"""Fresh clone file-only resume after generated-state deletion; no model/service."""
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/bank_contract.py"
CONTEXTS = ["projectbrief.md", "productContext.md", "activeContext.md", "systemPatterns.md", "techContext.md", "progress.md"]


class BankUsabilityContract(unittest.TestCase):
    def command(self, world, *args):
        result = world.run([sys.executable, "-I", HELPER, *args], cwd=world.root, check=False)
        return result, json.loads(result.stdout)

    def fixture(self, world):
        repo = world.repository()
        (repo / "README.md").write_text("# Fixture\n\nApproved intent: investigate retry behavior within this component.\n")
        context = {
            "projectbrief.md": {"body": "# Project brief\n\nGoal: investigate retry behavior. This is approved intent, not proof of implementation; [source](../README.md).\n",
                                "purpose": "Goals and evidence", "scope": "repository", "lifecycle": "active", "read-when": "When resuming the project"},
            "activeContext.md": {"body": "# Active context\n\nCurrent focus: retry component only.\n\nOpen question: applicability outside this component is unknown.\n\nNext: inspect the current test result; [intent](../README.md).\n",
                                 "purpose": "Current retry focus", "scope": "component:retry", "lifecycle": "active", "read-when": "When editing the retry component"},
            "progress.md": {"body": "# Historical progress\n\nExpired guidance for branch legacy; not current project guidance. RETIRED_GUIDANCE_CANARY\n",
                            "purpose": "Retired branch history", "scope": "branch:legacy", "lifecycle": "expired", "read-when": "Only when investigating that earlier branch"},
        }
        identity = str(uuid.uuid4())
        proposal = {"project-id": identity, "router-updated": "2026-10-22T18:00:00Z", "contexts": context,
                    "omitted": {name: "No distinct useful evidence supplied" for name in CONTEXTS if name not in context}}
        file = world.inside("router.json")
        file.write_text(json.dumps(proposal))
        result, rendered = self.command(world, "render", "--input", file)
        self.assertEqual(result.returncode, 0, rendered)
        bank = repo / "memory-bank"
        bank.mkdir()
        for name, body in rendered["files"].items():
            (bank / name).write_text(body)
        world.git("add", ".", cwd=repo)
        world.git("commit", "-m", "Useful canonical bank", cwd=repo)
        return repo, bank, identity

    def delete_generated_state(self, world):
        for name in ("config", "state", "data", "cache", "backups"):
            root = world.paths[name]
            (root / "disposable-memory-index.json").write_text('{"stale": "not canonical truth"}')
            shutil.rmtree(root)

    def bounded_read(self, path, maximum):
        with path.open("rb") as stream:
            body = stream.read(maximum + 1)
        self.assertLessEqual(len(body), maximum, "Narrow the read rather than silently truncating conditions")
        return body.decode("utf-8")

    def test_fresh_clone_resumes_from_bounded_current_markdown_without_any_generated_state(self):
        with World() as world:
            repo, bank, identity = self.fixture(world)
            source_bytes = {p.name: p.read_bytes() for p in bank.iterdir()}
            clone = world.clone(repo, "fresh clone")
            cloned_bank = clone / "memory-bank"
            self.delete_generated_state(world)
            result, receipt = self.command(world, "validate", "--repo", clone)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertEqual(receipt["project_id"], identity)
            self.assertFalse(receipt["writes_performed"])
            router = self.bounded_read(cloned_bank / "MEMORY.md", 16384)
            self.assertIn("component:retry", router)
            self.assertIn("branch:legacy", router)
            self.assertIn("expired", router)
            # This scenario's task is explicitly the retry component; the agent
            # follows its two current routes, not an invented ranking algorithm.
            current_routes = {r["path"]: r for r in receipt["routes"] if r["path"] in {"projectbrief.md", "activeContext.md"}}
            self.assertEqual(set(current_routes), {"projectbrief.md", "activeContext.md"})
            self.assertTrue(all(r["lifecycle"] == "active" for r in current_routes.values()))
            self.assertEqual(current_routes["activeContext.md"]["scope"], "component:retry")
            historical = next(r for r in receipt["routes"] if r["path"] == "progress.md")
            self.assertEqual(historical["lifecycle"], "expired")
            self.assertEqual(historical["scope"], "branch:legacy")
            bodies = [self.bounded_read(cloned_bank / name, 4096) for name in ("projectbrief.md", "activeContext.md")]
            assembled = "\n".join(bodies)
            self.assertIn("approved intent, not proof of implementation", assembled)
            self.assertIn("outside this component is unknown", assembled)
            self.assertIn("Next: inspect the current test result", assembled)
            self.assertNotIn("RETIRED_GUIDANCE_CANARY", assembled)
            self.assertTrue((clone / "README.md").is_file())
            self.assertEqual({p.name: p.read_bytes() for p in cloned_bank.iterdir()}, source_bytes)
            for name in ("config", "state", "data", "cache", "backups"):
                self.assertFalse(world.paths[name].exists())
            # A plain copy is still readable without Git metadata or an installed CLI.
            copy = world.inside("file-only copy")
            copy.mkdir()
            shutil.copytree(cloned_bank, copy / "memory-bank")
            self.assertFalse((copy / ".git").exists())
            self.assertIn("Current focus: retry component only", self.bounded_read(copy / "memory-bank/activeContext.md", 4096))
            result, checked = self.command(world, "validate", "--repo", copy)
            self.assertEqual(result.returncode, 0, checked)
            self.assertEqual(checked["project_id"], identity)

    def test_deleting_generated_state_does_not_make_an_interrupted_bank_usable_or_repair_it(self):
        with World() as world:
            repo, bank, identity = self.fixture(world)
            self.delete_generated_state(world)
            canonical = {p.name: p.read_bytes() for p in bank.iterdir()}
            for name, expected in ((".agent-memory-transaction.md", "Interrupted bank"), (".agent-memory.lock", "Busy bank")):
                marker = bank / name
                marker.write_text("Unknown interrupted operation; preserve for explicit inspection.\n")
                result, receipt = self.command(world, "validate", "--repo", repo)
                self.assertEqual(result.returncode, 1, receipt)
                self.assertFalse(receipt["valid"])
                self.assertIn(expected, receipt["error"])
                self.assertTrue(marker.exists())
                self.assertEqual({p.name: p.read_bytes() for p in bank.iterdir() if p.name in canonical}, canonical)
                marker.unlink()  # Fixture owner, not product age-based recovery.
            result, receipt = self.command(world, "validate", "--repo", repo)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertEqual(receipt["project_id"], identity)
            for name in ("config", "state", "data", "cache", "backups"):
                self.assertFalse(world.paths[name].exists())


if __name__ == "__main__":
    unittest.main()
