"""Public file-workflow acceptance for the router contract (#71)."""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from record_fixtures import record_text, project_id

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/bank_contract.py"
PORTABLE = ROOT / "skills/agent-memory/scripts/bank_contract.py"
PROJECT = "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd"
CONTEXTS = ["projectbrief.md", "productContext.md", "activeContext.md", "systemPatterns.md", "techContext.md", "progress.md"]


class RouterContract(unittest.TestCase):
    def proposal(self, expanded=False):
        supplied = CONTEXTS if expanded else ["projectbrief.md"]
        return {
            "project-id": PROJECT, "router-updated": "2026-10-22T17:00:00Z",
            "contexts": {name: {"body": f"# {name}\n\nUseful project context; see [source](../README.md).\n",
                                "purpose": "Orient the agent", "scope": "repository", "lifecycle": "active",
                                "read-when": "When resuming work"} for name in supplied},
            "omitted": {name: "No relevant evidence supplied yet" for name in CONTEXTS if name not in supplied},
        }

    def command(self, world, *args, helper=HELPER):
        result = world.run([sys.executable, "-I", helper, *args], check=False)
        return result, json.loads(result.stdout)

    def bank(self, world, expanded=False):
        proposal = world.inside("proposal.json")
        proposal.write_text(json.dumps(self.proposal(expanded)))
        result, receipt = self.command(world, "render", "--input", proposal)
        self.assertEqual(result.returncode, 0, receipt)
        self.assertFalse(receipt["writes_performed"])
        bank = world.paths["repo"] / receipt["bank_path"]
        self.assertFalse(bank.exists())  # rendering cannot bypass write guards
        bank.mkdir()
        for path, body in receipt["files"].items():
            (bank / path).write_text(body)
        return bank

    def validate(self, world):
        return self.command(world, "validate", "--repo", world.paths["repo"])

    def test_minimal_bank_resumes_from_only_canonical_files(self):
        with World() as world:
            bank = self.bank(world)
            for root in (world.paths["state"], world.paths["cache"], world.paths["backups"]):
                (root / "disposable").write_text("not authoritative")
                shutil.rmtree(root)
            before = {p.name: p.read_bytes() for p in bank.iterdir()}
            result, receipt = self.validate(world)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertEqual(receipt["project_id"], PROJECT)
            self.assertEqual([r["path"] for r in receipt["routes"]], ["projectbrief.md"])
            self.assertEqual(set(receipt["omitted"]), set(CONTEXTS) - {"projectbrief.md"})
            self.assertIn("[source](../README.md)", (bank / "projectbrief.md").read_text())
            self.assertEqual(before, {p.name: p.read_bytes() for p in bank.iterdir()})

    def test_expanded_bank_has_six_useful_contexts_and_independent_router_time(self):
        with World() as world:
            bank = self.bank(world, expanded=True)
            router_before = (bank / "MEMORY.md").read_bytes()
            (bank / "activeContext.md").write_text("# Current focus\n\nNext: verify a durable lesson.\n")
            result, receipt = self.validate(world)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertEqual(len(receipt["routes"]), 6)
            self.assertEqual(receipt["omitted"], {})
            self.assertEqual((bank / "MEMORY.md").read_bytes(), router_before)
            self.assertEqual(receipt["router_updated"], "2026-10-22T17:00:00Z")

    def test_render_refuses_empty_files_and_missing_omission_reasons(self):
        with World() as world:
            for corruption in ("empty", "missing-omission", "wrong-project", "wrong-time"):
                proposal = self.proposal()
                if corruption == "empty": proposal["contexts"]["projectbrief.md"]["body"] = "  "
                if corruption == "missing-omission": proposal["omitted"] = {}
                if corruption == "wrong-project": proposal["project-id"] = "project-name"
                if corruption == "wrong-time": proposal["router-updated"] = "2026-10-22"
                file = world.inside("proposal.json")
                file.write_text(json.dumps(proposal))
                result, receipt = self.command(world, "render", "--input", file)
                self.assertEqual(result.returncode, 1, receipt)
                self.assertFalse(receipt["writes_performed"])
            self.assertFalse((world.paths["repo"] / "memory-bank").exists())

    def test_byte_zero_schema_and_restricted_metadata_errors(self):
        with World() as world:
            bank = self.bank(world)
            router = bank / "MEMORY.md"
            original = router.read_text()
            bad = ["\ufeff" + original, "prefix\n" + original,
                   original.replace('"agent-memory/v1"', '"agent-memory/v999"'),
                   original.replace('schema: ', 'unknown: ', 1),
                   original.replace("---\n", "---\nschema: alias\n", 1),
                   original.replace('"agent-memory/v1"', '&alias something'),
                   original.replace('"agent-memory/v1"', '!tag something'),
                   original.replace('"agent-memory/v1"', '{"x": 1, "x": 2}'),
                   original.replace('"agent-memory/v1"', '"unterminated'),
                   original.replace('schema: "agent-memory/v1"', 'schema:\n  nested: unsupported')]
            for text in bad:
                router.write_text(text)
                result, receipt = self.validate(world)
                self.assertEqual(result.returncode, 1, receipt)
                self.assertFalse(receipt["valid"])

    def test_timezone_offsets_are_validated_without_normalizing_invalid_components(self):
        # Independent review caught datetime.fromisoformat normalizing +00:99.
        # Both public surfaces must enforce RFC3339 component ranges first.
        with World() as world:
            bank = self.bank(world)
            router = bank / "MEMORY.md"
            original = router.read_text()
            for offset in ("+00:99", "-00:60", "+24:00", "-24:00", "+99:00", "+01:99"):
                timestamp = "2026-10-22T17:00:00" + offset
                proposal = self.proposal()
                proposal["router-updated"] = timestamp
                file = world.inside("proposal.json")
                file.write_text(json.dumps(proposal))
                result, receipt = self.command(world, "render", "--input", file)
                self.assertEqual(result.returncode, 1, receipt)
                router.write_text(original.replace("2026-10-22T17:00:00Z", timestamp))
                result, receipt = self.validate(world)
                self.assertEqual(result.returncode, 1, receipt)
            for offset in ("Z", "+00:00", "-00:00", "+05:30", "-07:00", "+23:59", "-23:59"):
                timestamp = "2026-10-22T17:00:00" + offset
                proposal = self.proposal()
                proposal["router-updated"] = timestamp
                file.write_text(json.dumps(proposal))
                self.assertEqual(self.command(world, "render", "--input", file)[0].returncode, 0)
                router.write_text(original.replace("2026-10-22T17:00:00Z", timestamp))
                self.assertEqual(self.validate(world)[0].returncode, 0)

    def test_missing_empty_and_omitted_existing_targets_fail(self):
        with World() as world:
            bank = self.bank(world)
            target = bank / "projectbrief.md"
            original = target.read_text()
            target.unlink()
            self.assertEqual(self.validate(world)[0].returncode, 1)
            target.write_text(" \n")
            self.assertEqual(self.validate(world)[0].returncode, 1)
            target.write_text(original)
            (bank / "progress.md").write_text("Unrouted content")
            self.assertEqual(self.validate(world)[0].returncode, 1)

    def test_paths_cannot_route_runtime_state_outside_bank_or_personal_overlays(self):
        with World() as world:
            bank = self.bank(world)
            router = bank / "MEMORY.md"
            original = router.read_text()
            for path in ("../state.md", "/tmp/state.md", ".agent-memory-transaction.md", "personal/profile.md",
                         "topics/../../state.md", "topics//state.md", "topics/secret%2fstate.md", "topics/.runtime.md"):
                router.write_text(original.replace("[projectbrief.md](<projectbrief.md>)", f"[{path}](<{path}>)"))
                self.assertEqual(self.validate(world)[0].returncode, 1, path)
            router.write_text(original)
            (bank / "projectbrief.md").unlink()
            (bank / "projectbrief.md").symlink_to(world.root / "outside.md")
            (world.root / "outside.md").write_text("outside bank")
            self.assertEqual(self.validate(world)[0].returncode, 1)

    def test_optional_topic_is_readable_and_runtime_state_is_not_routed(self):
        with World() as world:
            bank = self.bank(world)
            router = bank / "MEMORY.md"
            (bank / "topics").mkdir()
            (bank / "topics/design notes.md").write_text(record_text(
                "# Design\n\nA conditional lesson; [ADR](../../docs/adr.md).\n",
                project_id(router.read_text()), 'branch:feature'))
            row = "| [topics/design notes.md](<topics/design notes.md>) | Design context | branch:feature | active | When editing the feature |\n"
            router.write_text(router.read_text().replace("\n## Omitted contexts", "\n" + row + "\n## Omitted contexts"))
            result, receipt = self.validate(world)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertEqual(receipt["routes"][-1]["scope"], "branch:feature")
            (bank / ".agent-memory.lock").write_text("writer")
            self.assertIn("Busy bank", self.validate(world)[1]["error"])
            (bank / ".agent-memory.lock").unlink()
            (bank / ".agent-memory-transaction.md").write_text("interrupted")
            self.assertIn("Interrupted bank", self.validate(world)[1]["error"])

    def test_router_bounds_duplicate_routes_and_orphan_files(self):
        with World() as world:
            bank = self.bank(world)
            router = bank / "MEMORY.md"
            original = router.read_text()
            row = next(line for line in original.splitlines() if "[projectbrief.md]" in line)
            cases = [original.replace(row, row + "\n" + row), original + "x" * 16384,
                     original.replace("(<projectbrief.md>)", "(<projectbrief.md)"),
                     original.replace("| repository |", "| component:../other |"),
                     original.replace(row, "\n".join(row.replace("projectbrief.md", f"topics/{n}.md") for n in range(65)))]
            for text in cases:
                router.write_text(text)
                self.assertEqual(self.validate(world)[0].returncode, 1)
            router.write_text(original)
            (bank / "topics").mkdir()
            (bank / "topics/orphan.md").write_text("Not reachable")
            self.assertEqual(self.validate(world)[0].returncode, 1)

    def test_optional_authority_anchor_uses_the_common_reader_and_canonical_identifiers(self):
        with World() as world:
            proposal = self.proposal()
            proposal["canonical-remote"] = "repo://github.com/owner/project"
            file = world.inside("proposal.json")
            file.write_text(json.dumps(proposal))
            result, receipt = self.command(world, "render", "--input", file)
            self.assertEqual(result.returncode, 0, receipt)
            bank = world.paths["repo"] / "memory-bank"
            bank.mkdir()
            for path, body in receipt["files"].items():
                (bank / path).write_text(body)
            self.assertEqual(self.validate(world)[0].returncode, 0)
            for remote in (None, "https://secret@github.com/owner/project", "repo://github.com/Owner/Project", "unknown"):
                proposal["canonical-remote"] = remote
                file.write_text(json.dumps(proposal))
                result, receipt = self.command(world, "render", "--input", file)
                self.assertEqual(result.returncode, 1, receipt)

    def test_portable_helper_works_after_copy_outside_checkout(self):
        with World() as world:
            pack = world.inside("portable pack")
            shutil.copytree(ROOT / "skills/agent-memory", pack)
            self.bank(world)
            result, receipt = self.command(world, "validate", "--repo", world.paths["repo"], helper=pack / "scripts/bank_contract.py")
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["valid"])


if __name__ == "__main__":
    unittest.main()
