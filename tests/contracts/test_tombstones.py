"""Content-free deletion markers and actual stale-projection rebuild canaries."""
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/tombstones.py"
RENDER = ROOT / "scripts/bank_contract.py"


class TombstonesContract(unittest.TestCase):
    def save(self, w, data, name="input.json"):
        p = w.inside(name)
        p.write_text(json.dumps(data))
        return p

    def command(self, w, *args, helper=HELPER):
        p = w.run([sys.executable, "-I", helper, *args], cwd=w.root, check=False)
        return p, json.loads(p.stdout)

    def metadata(self, project=None):
        return {"id": str(uuid.uuid4()), "scope": project or str(uuid.uuid4()), "deleted-at": "2026-10-22T18:00:00.123456789012345Z",
                "deletion-authority": str(uuid.uuid4()), "reason-class": "user-request"}

    def fixture(self, w):
        repo = w.repository()
        project = str(uuid.uuid4())
        proposal = json.loads((ROOT / "skills/agent-memory/templates/router-proposal.example.json").read_text())
        proposal["project-id"] = project
        p, rendered = self.command(w, "render", "--input", self.save(w, proposal), helper=RENDER)
        self.assertEqual(p.returncode, 0, rendered)
        bank = repo / "memory-bank"
        bank.mkdir()
        for name, body in rendered["files"].items():
            (bank / name).write_text(body)
        dead = self.metadata(project)
        old = bank / "topics" / (dead["id"] + ".md")
        old.parent.mkdir()
        old.write_text("# Lesson\n\nDELETED_SECRET_CANARY must never be rebuilt.\n")
        stale = {"project_id": project, "records": [{"id": dead["id"], "body": old.read_text(), "conditions": ["branch:old"]},
                  {"id": str(uuid.uuid4()), "body": "Useful surviving lesson", "conditions": ["component:api"],
                   "evidence": ["src/api.py"], "developer": str(uuid.uuid4())}]}
        # The fixture owner performs deletion; the governed forget command is a
        # separate story. This tests its tombstone format and rebuild boundary.
        old.unlink()
        p, rendered_dead = self.command(w, "render", "--input", self.save(w, dead, "deleted.json"))
        self.assertEqual(p.returncode, 0, rendered_dead)
        path = bank / rendered_dead["path"]
        path.parent.mkdir(parents=True)
        path.write_text(rendered_dead["content"])
        row = f'| [{rendered_dead["path"]}](<{rendered_dead["path"]}>) | Deletion guard | repository | deleted | Before rebuilding projections |\n'
        router = bank / "MEMORY.md"
        router.write_text(router.read_text().replace("\n## Omitted contexts\n", row + "\n## Omitted contexts\n"))
        return repo, bank, dead, path, stale

    def test_exact_five_field_format_roundtrips_without_personal_prose_or_generated_state(self):
        with World() as w:
            metadata = self.metadata()
            p, rendered = self.command(w, "render", "--input", self.save(w, metadata))
            self.assertEqual(p.returncode, 0, rendered)
            self.assertEqual(set(rendered["metadata"]), {"id", "scope", "deleted-at", "deletion-authority", "reason-class"})
            self.assertFalse(rendered["identity_authenticated"])
            self.assertFalse(rendered["writes_performed"])
            file = w.inside("tombstone.md")
            file.write_text(rendered["content"])
            p, checked = self.command(w, "validate", "--file", file)
            self.assertEqual(p.returncode, 0, checked)
            self.assertEqual(checked["metadata"], metadata)
            self.assertEqual(checked["path"], "history/tombstones/" + metadata["id"] + ".md")
            self.assertEqual(rendered["content"].split("\n---\n", 1)[1], "")
            self.assertFalse(Path(w.env["AGENT_MEMORY_CONFIG_HOME"]).exists())
            self.assertFalse(Path(w.env["AGENT_MEMORY_STATE_HOME"]).exists())

    def test_sensitive_extra_fields_bodies_names_and_fingerprints_refuse_without_echo_or_mutation(self):
        with World() as w:
            metadata = self.metadata()
            for key in ("body", "source", "evidence", "hash", "content-sha256", "email", "deleted-path", "title", "DELETED_SECRET_CANARY"):
                bad = {**metadata, key: "DELETED_SECRET_CANARY"}
                p, error = self.command(w, "render", "--input", self.save(w, bad))
                self.assertEqual(p.returncode, 1, key)
                self.assertNotIn("DELETED_SECRET_CANARY", p.stdout)
            for key in ("id", "scope", "deletion-authority", "reason-class", "deleted-at"):
                p, _ = self.command(w, "render", "--input", self.save(w, {**metadata, key: "DELETED_SECRET_CANARY"}))
                self.assertEqual(p.returncode, 1, key)
                self.assertNotIn("DELETED_SECRET_CANARY", p.stdout)
            p, rendered = self.command(w, "render", "--input", self.save(w, metadata))
            file = w.inside("bad-body.md")
            file.write_text(rendered["content"] + "DELETED_SECRET_CANARY\n")
            before = file.read_bytes()
            p, _ = self.command(w, "validate", "--file", file)
            self.assertEqual(p.returncode, 1)
            self.assertNotIn("DELETED_SECRET_CANARY", p.stdout)
            self.assertEqual(file.read_bytes(), before)

    def test_delete_rebuild_and_repeat_cannot_resurrect_id_under_changed_conditions(self):
        with World() as w:
            repo, bank, dead, file, stale = self.fixture(w)
            canonical = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob("*.md")}
            source = self.save(w, stale, "old-projection.json")
            original = source.read_bytes()
            outputs = []
            for conditions in (["branch:old"], ["repository"], ["component:other"]):
                stale["records"][0]["conditions"] = conditions
                p, guarded = self.command(w, "filter", "--repo", repo, "--input", self.save(w, stale, "rebuild-input.json"))
                self.assertEqual(p.returncode, 0, guarded)
                self.assertEqual(guarded["suppressed"], 1)
                self.assertEqual(guarded["records"], [stale["records"][1]])
                self.assertNotIn("DELETED_SECRET_CANARY", p.stdout)
                self.assertFalse(guarded["writes_performed"])
                outputs.append(guarded)
                rebuilt = w.inside("rebuilt-projection.json")
                rebuilt.write_text(json.dumps(guarded["records"]))
                self.assertNotIn("DELETED_SECRET_CANARY", rebuilt.read_text())
            self.assertEqual(outputs[0], outputs[1])
            self.assertEqual(outputs[1], outputs[2])
            self.assertEqual(source.read_bytes(), original)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob("*.md")}, canonical)
            self.assertNotIn("DELETED_SECRET_CANARY", file.read_text())
            self.assertFalse(Path(w.env["AGENT_MEMORY_CONFIG_HOME"]).exists())
            self.assertFalse(Path(w.env["AGENT_MEMORY_STATE_HOME"]).exists())

    def test_unknown_ids_wrong_project_invalid_history_and_interrupted_bank_stop_rebuild(self):
        with World() as w:
            repo, bank, dead, file, stale = self.fixture(w)
            for bad in ({**stale, "project_id": str(uuid.uuid4())}, {**stale, "records": [{"body": "DELETED_SECRET_CANARY"}]},
                        {**stale, "records": [stale["records"][0], stale["records"][0]]}):
                p, _ = self.command(w, "filter", "--repo", repo, "--input", self.save(w, bad))
                self.assertEqual(p.returncode, 1)
                self.assertNotIn("DELETED_SECRET_CANARY", p.stdout)
            original = file.read_bytes()
            file.write_text(file.read_text() + "DELETED_SECRET_CANARY\n")
            p, _ = self.command(w, "filter", "--repo", repo, "--input", self.save(w, stale))
            self.assertEqual(p.returncode, 1)
            self.assertNotIn("DELETED_SECRET_CANARY", p.stdout)
            file.write_bytes(original)
            for name in (".agent-memory-transaction.md", ".agent-memory.lock"):
                marker = bank / name
                marker.write_text("Do not guess or bypass interruption")
                p, _ = self.command(w, "filter", "--repo", repo, "--input", self.save(w, stale))
                self.assertEqual(p.returncode, 1)
                self.assertTrue(marker.exists())
                marker.unlink()

    def test_copied_portable_guard_uses_the_same_canonical_project_history(self):
        with World() as w:
            repo, bank, dead, file, stale = self.fixture(w)
            pack = w.inside("portable pack with spaces")
            shutil.copytree(ROOT / "skills/agent-memory", pack)
            p, guarded = self.command(w, "filter", "--repo", repo, "--input", self.save(w, stale), helper=pack / "scripts/tombstones.py")
            self.assertEqual(p.returncode, 0, guarded)
            self.assertEqual(guarded["suppressed"], 1)
            self.assertNotIn("DELETED_SECRET_CANARY", p.stdout)

    def test_history_namespace_route_state_and_record_identity_must_match_before_rebuild(self):
        with World() as w:
            repo, bank, dead, file, stale = self.fixture(w)
            original, router = file.read_bytes(), (bank / "MEMORY.md").read_bytes()
            for field in ("id", "scope"):
                changed = {**dead, field: str(uuid.uuid4())}
                p, rendered = self.command(w, "render", "--input", self.save(w, changed))
                self.assertEqual(p.returncode, 0, rendered)
                file.write_text(rendered["content"])
                p, _ = self.command(w, "filter", "--repo", repo, "--input", self.save(w, stale))
                self.assertEqual(p.returncode, 1)
                file.write_bytes(original)
            (bank / "MEMORY.md").write_bytes(router.replace(b"| repository | deleted |", b"| repository | active |"))
            p, _ = self.command(w, "filter", "--repo", repo, "--input", self.save(w, stale))
            self.assertEqual(p.returncode, 1)
            self.assertEqual(file.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
