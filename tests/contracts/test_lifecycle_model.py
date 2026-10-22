"""Public-command checks for the shared lifecycle and current predicate."""
import copy
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/lifecycle_model.py"
PACK = ROOT / "skills/agent-memory"
RECORD_ID = "c7063a92-f252-4d83-a728-3355018aca81"
OLD_EVENT = "737a69f9-e8d9-49d0-88c2-dda9942267a4"
NEW_EVENT = "441b9eaa-72a2-45fb-bbf2-57297d7228f4"


class LifecycleContract(unittest.TestCase):
    def command(self, world, operation, request=None, helper=HELPER):
        args = [sys.executable, "-I", helper, operation]
        if request is not None:
            file = world.inside("request.json")
            file.write_text(json.dumps(request))
            args += ["--input", file]
        result = world.run(args, check=False)
        return result, json.loads(result.stdout)

    def selection(self):
        return {
            "record": {"status": "active", "trust": "reviewed", "visibility": "visible", "confidence": 0.2,
                       "valid-from": "2026-01-01T00:00:00Z", "valid-until": "2027-01-01T00:00:00Z"},
            "context": {"authorized-scope": True, "applicable": True, "now": "2026-10-22T17:00:00Z",
                        "minimum-trust": "reviewed", "active-superseders": []},
        }

    def transition(self, status="active", operation="correct"):
        return {
            "record": {"id": RECORD_ID, "version": 1, "event-id": OLD_EVENT, "event-ids": [OLD_EVENT],
                       "evidence": ["git:old-commit"], "status": status, "valid-until": "2026-10-22T17:00:00Z"},
            "operation": operation,
            "event": {"id": NEW_EVENT, "evidence": ["git:new-commit"], "authority": "developer:operator",
                      "reason": "Stronger evidence", "reviewed": True, "now": "2026-10-22T17:00:00Z",
                      "replacement": {"id": RECORD_ID, "version": 2, "status": "active",
                                      "replaces": {"id": RECORD_ID, "version": 1}, "applicability-preserved": True}},
        }

    def test_generated_guide_definition_and_router_share_one_model(self):
        with World() as world:
            model = self.command(world, "definition")[1]["result"]
            guide = world.run([sys.executable, "-I", HELPER, "guide"]).stdout
            self.assertEqual(guide, (PACK / "model/GUIDE.md").read_text())
            self.assertEqual(set(model["states"]), {"candidate", "quarantined", "active", "superseded", "revoked", "expired", "deleted"})
            for state in model["states"]:
                self.assertIn(f"`{state}`", guide)
            for rule in model["current-predicate"]:
                self.assertIn(f"`{rule['gate']}`", guide)
            router = json.loads((PACK / "schemas/bank-router.json").read_text())
            self.assertEqual(router["x-lifecycle-model"], "lifecycle.json")
            self.assertNotIn("x-lifecycle-states", router)

    def test_current_requires_every_gate_before_ranking(self):
        with World() as world:
            request = self.selection()
            result, receipt = self.command(world, "current", request)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["result"]["current"])
            changes = [
                ("context", "authorized-scope", False, "authorized-scope"),
                ("context", "applicable", False, "applicable"),
                ("record", "status", "revoked", "active"),
                ("record", "trust", "untrusted", "sufficient-trust"),
                ("record", "visibility", "hidden", "visible"),
                ("context", "active-superseders", ["new-record/version-2"], "no-active-superseder"),
                ("record", "valid-until", "2026-10-22T17:00:00Z", "valid-time"),
            ]
            for area, key, value, reason in changes:
                candidate = copy.deepcopy(request)
                candidate[area][key] = value
                decision = self.command(world, "current", candidate)[1]["result"]
                self.assertFalse(decision["current"])
                self.assertIn(reason, decision["reasons"])
            # High confidence cannot override weak trust; low confidence is not
            # a hidden alias for a revoked/invisible lifecycle.
            request["record"].update(confidence=1.0, trust="untrusted")
            self.assertFalse(self.command(world, "current", request)[1]["result"]["current"])
            request["record"].update(confidence=0.0, trust="verified")
            self.assertTrue(self.command(world, "current", request)[1]["result"]["current"])

    def test_time_is_start_inclusive_end_exclusive_with_offsets(self):
        with World() as world:
            request = self.selection()
            request["record"].update({"valid-from": "2026-10-22T10:00:00-07:00", "valid-until": "2026-10-22T17:00:01Z"})
            self.assertTrue(self.command(world, "current", request)[1]["result"]["current"])
            request["context"]["now"] = "2026-10-22T17:00:01Z"
            self.assertFalse(self.command(world, "current", request)[1]["result"]["current"])
            request["record"].update({"valid-from": None, "valid-until": None})
            self.assertTrue(self.command(world, "current", request)[1]["result"]["current"])

    def test_unknown_or_malformed_axes_do_not_become_current(self):
        with World() as world:
            invalid = [("record", "status", "invented"), ("record", "trust", "certain"),
                       ("record", "visibility", "personal"), ("record", "confidence", True),
                       ("record", "confidence", 1.1), ("record", "confidence", 10 ** 400), ("context", "authorized-scope", "yes"),
                       ("context", "applicable", None), ("context", "minimum-trust", None),
                       ("context", "now", "2026-10-22T17:00:00+00:99"),
                       ("context", "now", "2026-10-22T17:00:00.١Z"),
                       ("context", "active-superseders", None),
                       ("record", "valid-until", "2025-01-01T00:00:00Z")]
            for area, key, value in invalid:
                request = self.selection()
                request[area][key] = value
                result, receipt = self.command(world, "current", request)
                self.assertEqual(result.returncode, 1, receipt)
                self.assertIn("error", receipt)

    def test_fractional_boundaries_preserve_every_accepted_digit(self):
        # Independent review caught datetime truncating accepted fractions and
        # admitting a future record / prematurely expiring a still-valid one.
        with World() as world:
            request = self.selection()
            request["record"].update({"valid-from": "2026-10-22T17:00:00.0000001Z", "valid-until": None})
            request["context"]["now"] = "2026-10-22T17:00:00.0000000Z"
            self.assertFalse(self.command(world, "current", request)[1]["result"]["current"])
            request["record"].update({"valid-from": None, "valid-until": "2026-10-22T17:00:00.0000009Z"})
            request["context"]["now"] = "2026-10-22T17:00:00.0000001Z"
            self.assertTrue(self.command(world, "current", request)[1]["result"]["current"])
            request["context"]["now"] = "2026-10-22T17:00:00.000000900000Z"
            self.assertFalse(self.command(world, "current", request)[1]["result"]["current"])
            # UTC offset equivalence and much longer fractions stay exact too.
            fraction = "0" * 50 + "1"
            request["record"].update({"valid-from": "2026-10-22T10:00:00." + fraction + "-07:00", "valid-until": None})
            request["context"]["now"] = "2026-10-22T17:00:00Z"
            self.assertFalse(self.command(world, "current", request)[1]["result"]["current"])
            request["context"]["now"] = "2026-10-22T17:00:00." + fraction + "00Z"
            self.assertTrue(self.command(world, "current", request)[1]["result"]["current"])
            expiry = self.transition(operation="expire")
            expiry["record"]["valid-until"] = "2026-10-22T17:00:00.0000009Z"
            expiry["event"]["now"] = "2026-10-22T17:00:00.0000001Z"
            self.assertFalse(self.command(world, "transition", expiry)[1]["result"]["legal"])
            expiry["event"]["now"] = "2026-10-22T10:00:00.000000900-07:00"
            self.assertTrue(self.command(world, "transition", expiry)[1]["result"]["legal"])

    def test_transition_table_drives_all_legal_states_and_deletion_is_terminal(self):
        with World() as world:
            model = self.command(world, "definition")[1]["result"]
            for rule in model["transitions"]:
                for status in rule["from"]:
                    request = self.transition(status, rule["operation"])
                    result, receipt = self.command(world, "transition", request)
                    self.assertEqual(result.returncode, 0, receipt)
                    self.assertTrue(receipt["result"]["legal"], receipt)
                    self.assertEqual(receipt["result"]["to"], rule["to"])
            for rule in model["transitions"]:
                request = self.transition("deleted", rule["operation"])
                decision = self.command(world, "transition", request)[1]["result"]
                self.assertFalse(decision["legal"])
                self.assertEqual(decision["reasons"], ["terminal-deletion"])

    def test_correction_and_supersession_require_exact_active_replacement(self):
        with World() as world:
            request = self.transition()
            decision = self.command(world, "transition", request)[1]["result"]
            self.assertTrue(decision["creates_new_version"])
            self.assertEqual(decision["prior_version_status"], "superseded")
            self.assertEqual(decision["new_version_status"], "active")
            for key, value in [("status", "candidate"), ("version", 1), ("replaces", {"id": RECORD_ID, "version": 9}),
                               ("applicability-preserved", False)]:
                bad = copy.deepcopy(request)
                bad["event"]["replacement"][key] = value
                self.assertFalse(self.command(world, "transition", bad)[1]["result"]["legal"])
            # Supersession can name another record; correction retains identity.
            request["event"]["replacement"]["id"] = NEW_EVENT
            self.assertFalse(self.command(world, "transition", request)[1]["result"]["legal"])
            request["operation"] = "supersede"
            self.assertTrue(self.command(world, "transition", request)[1]["result"]["legal"])

    def test_revocation_expiry_and_reactivation_remain_distinct(self):
        with World() as world:
            request = self.transition(operation="revoke")
            del request["event"]["replacement"]
            request["event"]["evidence"] = []
            decision = self.command(world, "transition", request)[1]["result"]
            self.assertTrue(decision["legal"])
            self.assertEqual(decision["to"], "revoked")
            expiry = self.transition(operation="expire")
            expiry["event"]["now"] = "2026-10-22T16:59:59Z"
            self.assertFalse(self.command(world, "transition", expiry)[1]["result"]["legal"])
            expiry["event"]["now"] = "2026-10-22T17:00:00Z"
            self.assertTrue(self.command(world, "transition", expiry)[1]["result"]["legal"])
            for status in ("superseded", "revoked", "expired"):
                request = self.transition(status, "reactivate")
                decision = self.command(world, "transition", request)[1]["result"]
                self.assertTrue(decision["legal"])
                self.assertEqual(decision["prior_version_status"], status)
                self.assertEqual(decision["new_version_status"], "active")
                request["event"]["evidence"] = request["record"]["evidence"]
                self.assertFalse(self.command(world, "transition", request)[1]["result"]["legal"])
                request = self.transition(status, "reactivate")
                request["event"]["id"] = OLD_EVENT
                self.assertFalse(self.command(world, "transition", request)[1]["result"]["legal"])

    def test_each_event_guard_refuses_missing_authority_review_or_new_event(self):
        with World() as world:
            for operation, field in (("correct", "authority"), ("revoke", "reason"), ("admit", "reviewed"), ("delete", "id")):
                request = self.transition("candidate" if operation == "admit" else "active", operation)
                del request["event"][field]
                self.assertFalse(self.command(world, "transition", request)[1]["result"]["legal"])
            request = self.transition()
            request["record"]["event-ids"].append(NEW_EVENT)
            self.assertIn("new-event", self.command(world, "transition", request)[1]["result"]["reasons"])

    def test_copied_portable_evaluator_does_not_modify_inputs_or_create_state(self):
        with World() as world:
            pack = world.inside("copied pack")
            shutil.copytree(PACK, pack)
            request = self.selection()
            file = world.inside("request.json")
            original = json.dumps(request)
            file.write_text(original)
            helper = pack / "scripts/lifecycle_model.py"
            result, receipt = self.command(world, "current", request, helper=helper)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertFalse(receipt["writes_performed"])
            self.assertEqual(json.loads(file.read_text()), request)
            for name in ("state", "cache", "bank", "config"):
                self.assertEqual(list(world.paths[name].iterdir()), [])


if __name__ == "__main__":
    unittest.main()
