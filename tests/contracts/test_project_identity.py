"""Public project identity operations against real repositories and worktrees."""
import json
import os
from pathlib import Path
import shutil
import socket
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/project_identity.py"
BANK = ROOT / "scripts/bank_contract.py"
CONTEXTS = ["projectbrief.md", "productContext.md", "activeContext.md", "systemPatterns.md", "techContext.md", "progress.md"]


class ProjectIdentityContract(unittest.TestCase):
    def command(self, world, *args, helper=HELPER):
        result = world.run([sys.executable, "-I", helper, *args], cwd=world.root, check=False)
        return result, json.loads(result.stdout)

    def preview(self, world, repo, action, **values):
        args = ["preview", "--repo", repo, "--action", action, "--reason", "Explicit fixture decision"]
        for key, value in values.items():
            args.extend(["--" + key.replace("_", "-"), value])
        result, receipt = self.command(world, *args)
        self.assertEqual(result.returncode, 0, receipt)
        self.assertFalse(receipt["writes_performed"])
        return receipt

    def apply(self, world, proposal):
        file = world.inside("project-proposal.json")
        file.write_text(json.dumps(proposal))
        return self.command(world, "apply", "--proposal", file)

    def registry(self, world):
        return Path(world.env["AGENT_MEMORY_CONFIG_HOME"]) / "projects.json"

    def bank(self, world, repo, identity, remote=None):
        proposal = {"project-id": identity, "router-updated": "2026-10-22T18:00:00Z",
                    "contexts": {"projectbrief.md": {"body": "# Project\n\nFixture context with [source](../README.md).",
                                 "purpose": "Goals", "scope": "repository", "lifecycle": "active", "read-when": "When starting"}},
                    "omitted": {key: "No relevant evidence supplied" for key in CONTEXTS[1:]}}
        if remote is not None:
            proposal["canonical-remote"] = remote
        file = world.inside("bank-proposal.json")
        file.write_text(json.dumps(proposal))
        result = world.run([sys.executable, "-I", BANK, "render", "--input", file], cwd=world.root)
        bank = repo / "memory-bank"
        bank.mkdir(exist_ok=True)
        for name, body in json.loads(result.stdout)["files"].items():
            (bank / name).write_text(body)

    def create(self, world, repo, **values):
        proposal = self.preview(world, repo, "create", **values)
        result, receipt = self.apply(world, proposal)
        self.assertEqual(result.returncode, 0, receipt)
        self.assertTrue(receipt["verified"])
        self.assertFalse(receipt["binding_status"]["resolved"])
        target = proposal["proposal"]["bank_identity"]
        self.bank(world, repo, target["project-id"], target["canonical-remote"])
        return target["project-id"], proposal

    def status(self, world, repo):
        result, receipt = self.command(world, "status", "--repo", repo)
        self.assertEqual(result.returncode, 0, receipt)
        return receipt

    def commit(self, world, repo):
        world.git("add", ".", cwd=repo)
        world.git("commit", "-m", "Fixture bank", cwd=repo)

    def historical(self, world, repo):
        data = json.loads(self.registry(world).read_text())
        current = self.status(world, repo)['evidence']['checkout_key']
        device, inode = current.split(':')
        historical = str(int(device) + 100) + ':' + inode
        data['checkouts'][historical] = data['checkouts'].pop(current)
        self.registry(world).write_text(json.dumps(data))
        return current, historical

    def test_device_change_recovers_readonly_and_explicit_link_persists_alias(self):
        with World() as world:
            repo = world.repository()
            world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
            identity, _ = self.create(world, repo)
            current, old = self.historical(world, repo)
            before = self.registry(world).read_bytes()
            status = self.status(world, repo)
            self.assertTrue(status['resolved'], status)
            self.assertEqual(status['project_id'], identity)
            self.assertEqual(status['evidence']['checkout_key'], old)
            self.assertEqual(status['evidence']['observed_checkout_key'], current)
            self.assertEqual(status['evidence']['binding_source'], 'historical-device-change')
            self.assertEqual(self.registry(world).read_bytes(), before)
            previous_history = status['checkout_entry']['history']
            proposal = self.preview(world, repo, 'link')
            result, receipt = self.apply(world, proposal)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt['binding_status']['resolved'])
            self.assertEqual(receipt['binding_status']['evidence']['binding_source'], 'exact')
            data = json.loads(self.registry(world).read_text())
            self.assertEqual(data['checkouts'][current]['history'][:-1], previous_history)
            self.assertIn(old, data['checkouts'])
            self.commit(world, repo)
            worktree = world.inside('recovered worktree')
            world.git('worktree', 'add', '-b', 'recovered', worktree, cwd=repo)
            self.assertTrue(self.status(world, worktree)['resolved'])

    def test_scope_link_repair_preserves_bank_and_verifies_current_alias(self):
        from contracts.test_scope_binding import ScopeBindingContract
        with World() as world:
            helper = ScopeBindingContract()
            repo = world.repository()
            world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
            helper.developer(world)
            self.create(world, repo)
            current, old = self.historical(world, repo)
            before = helper.canonical(repo / 'memory-bank')
            proposal = helper.preview(world, repo, 'link')
            result, receipt = helper.apply(world, proposal)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt['verified'], receipt)
            self.assertTrue(helper.status(world, repo)['resolved'])
            self.assertEqual(helper.canonical(repo / 'memory-bank'), before)
            self.assertEqual(self.status(world, repo)['evidence']['checkout_key'], current)

    def test_historical_fork_and_rebind_require_explicit_alias_link(self):
        with World() as world:
            repo = world.repository()
            world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
            self.create(world, repo)
            self.historical(world, repo)
            before = self.registry(world).read_bytes()
            for action in ('rebind', 'fork'):
                result, receipt = self.command(world, 'preview', '--repo', repo,
                                               '--action', action, '--reason', 'Explicit fixture decision')
                self.assertEqual(result.returncode, 1, receipt)
                self.assertIn('explicit link', receipt['error'])
                self.assertEqual(self.registry(world).read_bytes(), before)
            self.apply(world, self.preview(world, repo, 'link'))
            self.preview(world, repo, 'rebind')
            world.git('remote', 'set-url', 'origin', 'https://github.com/example/fork.git', cwd=repo)
            self.preview(world, repo, 'fork')

    def test_observed_alias_registered_after_preview_refuses_and_retains_history(self):
        with World() as world:
            repo = world.repository()
            world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
            self.create(world, repo)
            current, old = self.historical(world, repo)
            proposal = self.preview(world, repo, 'link')
            data = json.loads(self.registry(world).read_text())
            data['checkouts'][current] = json.loads(json.dumps(data['checkouts'][old]))
            data['checkouts'][current]['locations'].append(str(world.inside('concurrent-known-location')))
            self.registry(world).write_text(json.dumps(data))
            before = self.registry(world).read_bytes()
            result, receipt = self.apply(world, proposal)
            self.assertEqual(result.returncode, 1, receipt)
            self.assertIn('changed after preview', receipt['error'])
            self.assertEqual(self.registry(world).read_bytes(), before)

    def test_old_pending_proposal_evidence_requires_fresh_preview(self):
        with World() as world:
            repo = world.repository()
            self.create(world, repo)
            proposal = self.preview(world, repo, 'link')
            for key in ('binding_source', 'observed_checkout_key', 'continuity_issues'):
                del proposal['proposal']['evidence'][key]
            before = self.registry(world).read_bytes()
            result, receipt = self.apply(world, proposal)
            self.assertEqual(result.returncode, 1, receipt)
            self.assertIn('evidence changed', receipt['error'])
            self.assertEqual(self.registry(world).read_bytes(), before)

    def test_historical_candidates_are_deterministic_and_conflicts_refused(self):
        with World() as world:
            repo = world.repository()
            world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
            self.create(world, repo)
            current, old = self.historical(world, repo)
            data = json.loads(self.registry(world).read_text())
            replacement = '100:' + old.split(':')[1]
            data['checkouts'][replacement] = data['checkouts'].pop(old)
            old = replacement
            alternate = '9:' + old.split(':')[1]
            data['checkouts'][alternate] = json.loads(json.dumps(data['checkouts'][old]))
            self.registry(world).write_text(json.dumps(data))
            self.assertEqual(self.status(world, repo)['evidence']['checkout_key'], alternate)
            data['checkouts'][alternate]['authority_name'] = 'other'
            self.registry(world).write_text(json.dumps(data))
            status = self.status(world, repo)
            self.assertFalse(status['resolved'])
            self.assertIn('Conflicting historical', ' '.join(status['issues']))

    def test_conflicting_historical_aliases_allow_explicit_scope_link(self):
        from contracts.test_scope_binding import ScopeBindingContract
        with World() as world:
            helper = ScopeBindingContract()
            repo = world.repository()
            world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
            helper.developer(world)
            self.create(world, repo)
            current, old = self.historical(world, repo)
            data = json.loads(self.registry(world).read_text())
            other = '999999999:' + old.split(':')[1]
            data['checkouts'][other] = json.loads(json.dumps(data['checkouts'][old]))
            data['checkouts'][other]['authority_name'] = 'other'
            self.registry(world).write_text(json.dumps(data))
            self.assertFalse(self.status(world, repo)['resolved'])
            before_bank = helper.canonical(repo / 'memory-bank')
            result, receipt = helper.apply(world, helper.preview(world, repo, 'link'))
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt['verified'])
            self.assertTrue(self.status(world, repo)['resolved'])
            self.assertEqual(helper.canonical(repo / 'memory-bank'), before_bank)
            final = json.loads(self.registry(world).read_text())
            self.assertEqual(final['checkouts'][old], data['checkouts'][old])
            self.assertEqual(final['checkouts'][other], data['checkouts'][other])

    def test_rebound_aliases_after_later_device_change_require_explicit_selection(self):
        with World() as world:
            repo = world.repository()
            world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
            world.git('remote', 'add', 'upstream', 'https://github.com/example/new.git', cwd=repo)
            identity, _ = self.create(world, repo, authority='origin')
            current, old = self.historical(world, repo)
            result, receipt = self.apply(world, self.preview(world, repo, 'link', authority='origin'))
            self.assertEqual(result.returncode, 0, receipt)
            proposal = self.preview(world, repo, 'rebind', authority='upstream')
            result, receipt = self.apply(world, proposal)
            self.assertEqual(result.returncode, 0, receipt)
            self.bank(world, repo, identity, 'repo://github.com/example/new')
            data = json.loads(self.registry(world).read_text())
            later = str(int(current.split(':')[0]) + 200) + ':' + current.split(':')[1]
            data['checkouts'][later] = data['checkouts'].pop(current)
            self.registry(world).write_text(json.dumps(data))
            status = self.status(world, repo)
            self.assertFalse(status['resolved'])
            self.assertIn('Conflicting historical', ' '.join(status['issues']))
            result, receipt = self.apply(world, self.preview(world, repo, 'link', authority='upstream'))
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(self.status(world, repo)['resolved'])
            self.assertEqual(self.status(world, repo)['project_id'], identity)

    def test_historical_recovery_requires_bank_authority_location_and_inode(self):
        for change in ('remote', 'bank', 'inode', 'location', 'missing-remote'):
            with self.subTest(change=change), World() as world:
                repo = world.repository()
                world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
                self.create(world, repo)
                current, old = self.historical(world, repo)
                data = json.loads(self.registry(world).read_text())
                if change == 'remote':
                    world.git('remote', 'set-url', 'origin', 'https://github.com/example/other.git', cwd=repo)
                elif change == 'bank':
                    self.bank(world, repo, str(uuid.uuid4()), 'repo://github.com/example/project')
                elif change == 'inode':
                    data['checkouts'][old.split(':')[0] + ':999999999999'] = data['checkouts'].pop(old)
                elif change == 'location':
                    data['checkouts'][old]['locations'] = [str(world.inside('another/.git'))]
                else:
                    world.git('remote', 'remove', 'origin', cwd=repo)
                self.registry(world).write_text(json.dumps(data))
                self.assertFalse(self.status(world, repo)['resolved'])

    def test_historical_recovery_preserves_explicit_authority(self):
        with World() as world:
            repo = world.repository()
            world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
            world.git('remote', 'add', 'upstream', 'https://github.com/example/upstream.git', cwd=repo)
            self.create(world, repo, authority='origin')
            self.historical(world, repo)
            status = self.status(world, repo)
            self.assertTrue(status['resolved'], status)
            self.assertEqual(status['evidence']['authority_name'], 'origin')

    def test_bootstrap_is_read_only_until_explicit_local_registration_and_bank_initialization(self):
        with World() as world:
            repo = world.repository()
            status = self.status(world, repo)
            self.assertIsNone(status["project_id"])
            self.assertFalse(status["resolved"])
            self.assertFalse(self.registry(world).parent.exists())
            proposal = self.preview(world, repo, "create")
            self.assertFalse(self.registry(world).parent.exists())
            identity = proposal["project_id"]
            self.assertEqual(uuid.UUID(identity).version, 4)
            result, receipt = self.apply(world, proposal)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["verified"])
            self.assertEqual(receipt["changed_memory_paths"], [])
            self.assertFalse((repo / "memory-bank").exists())
            self.assertIn("pending", " ".join(receipt["binding_status"]["issues"]))
            self.bank(world, repo, identity)
            self.assertTrue(self.status(world, repo)["resolved"])
            self.assertFalse((self.registry(world).parent / "developers.json").exists())

    def test_normalization_strips_secrets_and_unifies_only_documented_equivalence(self):
        with World() as world:
            expected = "repo://github.com/owner/repo"
            same = ["https://alice:secret@GITHUB.COM/Owner/Repo.git/?token=secret#secret",
                    "git@github.com:Owner/Repo.git", "ssh://git@GITHUB.com:22/OWNER/REPO.git/",
                    "https://github.com:443/owner/repo", "http://github.com:80/owner/repo.git", expected,
                    "https://github.com/%6fwner/repo.git"]
            for remote in same:
                result, receipt = self.command(world, "normalize", remote)
                self.assertEqual(result.returncode, 0, receipt)
                self.assertEqual(receipt["canonical_remote"], expected)
                self.assertNotIn("secret", json.dumps(receipt))
            distinct = ["https://other.example/owner/repo.git", "https://github.com/another/repo.git",
                        "https://github.com/owner/fork.git", "ssh://git@github.com:2222/owner/repo.git",
                        "https://Example.com/Owner/Repo.git", "https://example.com/owner/repo.git"]
            results = [self.command(world, "normalize", value)[1]["canonical_remote"] for value in distinct]
            self.assertEqual(len(set(results + [expected])), len(results) + 1)
            for remote in ["https://secret@host/owner/%2Frepo", "ext::secret", "https://secret@host/a/../b", "ssh://secret@host:wrong/repo"]:
                result, receipt = self.command(world, "normalize", remote)
                self.assertEqual(result.returncode, 1, receipt)
                self.assertNotIn("secret", json.dumps(receipt))

    def test_linked_worktrees_join_via_the_real_common_git_directory(self):
        with World() as world:
            repo = world.repository()
            identity, _ = self.create(world, repo)
            self.commit(world, repo)
            worktree = world.inside("feature worktree")
            world.git("worktree", "add", "-b", "feature", worktree, cwd=repo)
            a, b = self.status(world, repo), self.status(world, worktree)
            self.assertTrue(b["resolved"], b)
            self.assertEqual(a["project_id"], b["project_id"])
            self.assertEqual(a["evidence"]["checkout_key"], b["evidence"]["checkout_key"])
            self.assertEqual(b["project_id"], identity)
            self.assertNotEqual(a["evidence"]["repo_root"], b["evidence"]["repo_root"])
            self.assertEqual(len(json.loads(self.registry(world).read_text())["checkouts"]), 1)

    def test_no_remote_move_keeps_id_and_preserves_location_history(self):
        with World() as world:
            repo = world.repository()
            identity, original_proposal = self.create(world, repo)
            old = str(repo / ".git")
            moved = world.inside("moved repository")
            repo.rename(moved)
            status = self.status(world, moved)
            self.assertTrue(status["resolved"], status)
            self.assertEqual(status["project_id"], identity)
            proposal = self.preview(world, moved, "link")
            self.assertEqual(self.apply(world, proposal)[0].returncode, 0)
            entry = self.status(world, moved)["checkout_entry"]
            self.assertEqual(entry["locations"], [old, str(moved / ".git")])
            self.assertEqual(entry["history"][-1]["action"], "link")
            result, replay = self.apply(world, original_proposal)
            self.assertEqual(result.returncode, 0, replay)
            self.assertTrue(replay["already_applied"])
            self.assertFalse(replay["binding_status"]["resolved"])  # Old location moved, registry success stays known.

    def test_copied_bank_requires_explicit_link_or_fork(self):
        with World() as world:
            repo = world.repository()
            identity, _ = self.create(world, repo)
            self.commit(world, repo)
            copied = world.clone(repo, "independent-copy")
            # A local Git clone creates origin; make this a no-remote copy fixture.
            world.git("remote", "remove", "origin", cwd=copied)
            self.assertFalse(self.status(world, copied)["resolved"])
            result, receipt = self.command(world, "preview", "--repo", copied, "--action", "bind", "--reason", "Copy")
            self.assertEqual(result.returncode, 1, receipt)
            proposal = self.preview(world, copied, "link")
            self.assertEqual(self.apply(world, proposal)[0].returncode, 0)
            self.assertTrue(self.status(world, copied)["resolved"])
            self.assertEqual(self.status(world, copied)["project_id"], identity)
            fork = self.preview(world, copied, "fork")
            result, receipt = self.apply(world, fork)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertFalse(receipt["binding_status"]["resolved"])
            new_id = fork["project_id"]
            self.assertNotEqual(new_id, identity)
            self.bank(world, copied, new_id)
            self.assertTrue(self.status(world, copied)["resolved"])
            self.assertTrue(self.status(world, repo)["resolved"])
            history = self.status(world, copied)["checkout_entry"]["history"]
            self.assertEqual(history[-1]["previous_project_id"], identity)
            self.assertEqual(len(json.loads(self.registry(world).read_text())["projects"]), 2)

    def test_fork_on_fresh_machine_is_caught_by_shared_authority_evidence(self):
        with World() as world:
            repo = world.repository()
            original = "repo://github.com/owner/project"
            identity = str(uuid.uuid4())
            self.bank(world, repo, identity, original)
            world.git("remote", "add", "origin", "git@github.com:other-developer/project.git", cwd=repo)
            self.assertFalse(self.status(world, repo)["resolved"])
            result, receipt = self.command(world, "preview", "--repo", repo, "--action", "bind", "--reason", "Automatic adoption")
            self.assertEqual(result.returncode, 1, receipt)
            self.assertIn("conflicts", receipt["error"])
            fork = self.preview(world, repo, "fork")
            self.assertEqual(self.apply(world, fork)[0].returncode, 0)
            target = fork["proposal"]["bank_identity"]
            self.bank(world, repo, target["project-id"], target["canonical-remote"])
            self.assertTrue(self.status(world, repo)["resolved"])
            self.assertNotEqual(identity, self.status(world, repo)["project_id"])

    def test_same_remote_on_fresh_machine_binds_stable_bank_id(self):
        with World() as world:
            repo = world.repository()
            identity = str(uuid.uuid4())
            self.bank(world, repo, identity, "repo://github.com/owner/project")
            world.git("remote", "add", "origin", "https://credentials:secret@GITHUB.COM/Owner/Project.git?secret=secret", cwd=repo)
            proposal = self.preview(world, repo, "bind")
            result, receipt = self.apply(world, proposal)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["binding_status"]["resolved"])
            self.assertEqual(receipt["project_id"], identity)
            self.assertNotIn("secret", self.registry(world).read_text())
            self.assertNotIn("secret", json.dumps(receipt))

    def test_multiple_remotes_require_authority_and_configured_selection_wins(self):
        with World() as world:
            repo = world.repository()
            world.git("remote", "add", "origin", "https://github.com/owner/project.git", cwd=repo)
            world.git("remote", "add", "upstream", "git@github.com:other/project.git", cwd=repo)
            self.assertFalse(self.status(world, repo)["resolved"])
            result, _ = self.command(world, "preview", "--repo", repo, "--action", "create", "--reason", "No authority")
            self.assertEqual(result.returncode, 1)
            world.git("config", "agent-memory.authorityRemote", "upstream", cwd=repo)
            identity, _ = self.create(world, repo)
            status = self.status(world, repo)
            self.assertTrue(status["resolved"], status)
            self.assertEqual(status["evidence"]["canonical_remote"], "repo://github.com/other/project")
            self.assertEqual(status["project_id"], identity)
            world.git("config", "agent-memory.authorityRemote", "origin", cwd=repo)
            self.assertFalse(self.status(world, repo)["resolved"])

    def test_explicit_authority_is_retained_and_conflicting_push_url_refuses(self):
        with World() as world:
            repo = world.repository()
            world.git("remote", "add", "origin", "https://github.com/owner/project.git", cwd=repo)
            world.git("remote", "add", "other", "https://github.com/other/project.git", cwd=repo)
            self.create(world, repo, authority="origin")
            self.assertTrue(self.status(world, repo)["resolved"])
            world.git("remote", "set-url", "--push", "origin", "https://github.com/other/project.git", cwd=repo)
            self.assertFalse(self.status(world, repo)["resolved"])
            result, receipt = self.command(world, "preview", "--repo", repo, "--action", "rebind", "--reason", "Conflicting publication destination")
            self.assertEqual(result.returncode, 1, receipt)

    def test_remote_change_stops_until_explicit_rebind_and_retains_alias_history(self):
        with World() as world:
            repo = world.repository()
            world.git("remote", "add", "origin", "https://github.com/old/project.git", cwd=repo)
            identity, old_proposal = self.create(world, repo)
            before_bank = (repo / "memory-bank/MEMORY.md").read_bytes()
            world.git("remote", "set-url", "origin", "git@github.com:new/project.git", cwd=repo)
            status = self.status(world, repo)
            self.assertFalse(status["resolved"])
            self.assertIn("Unexpected authority", " ".join(status["issues"]))
            rebind = self.preview(world, repo, "rebind")
            result, receipt = self.apply(world, rebind)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertFalse(receipt["binding_status"]["resolved"])
            self.assertEqual((repo / "memory-bank/MEMORY.md").read_bytes(), before_bank)
            self.bank(world, repo, identity, rebind["proposal"]["bank_identity"]["canonical-remote"])
            status = self.status(world, repo)
            self.assertTrue(status["resolved"], status)
            self.assertEqual(status["project_entry"]["aliases"], ["repo://github.com/old/project"])
            self.assertEqual(status["project_entry"]["history"][-1]["action"], "rebind")
            before = self.registry(world).read_bytes()
            result, receipt = self.apply(world, old_proposal)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["already_applied"])
            self.assertEqual(self.registry(world).read_bytes(), before)
            self.assertTrue(self.status(world, repo)["resolved"])

    def test_changed_registry_bank_remote_or_authority_refuses_stale_proposals(self):
        with World() as world:
            repo = world.repository()
            world.git("remote", "add", "origin", "https://github.com/owner/project.git", cwd=repo)
            proposal = self.preview(world, repo, "create")
            world.git("remote", "set-url", "origin", "https://github.com/other/project.git", cwd=repo)
            self.assertEqual(self.apply(world, proposal)[0].returncode, 1)
            self.assertFalse(self.registry(world).exists())
            proposal = self.preview(world, repo, "create")
            self.bank(world, repo, str(uuid.uuid4()), "repo://github.com/other/project")
            self.assertEqual(self.apply(world, proposal)[0].returncode, 1)
            proposal = self.preview(world, repo, "bind")
            world.git("config", "agent-memory.authorityRemote", "origin", cwd=repo)
            self.assertEqual(self.apply(world, proposal)[0].returncode, 1)
            proposal = self.preview(world, repo, "bind")
            self.assertEqual(self.apply(world, proposal)[0].returncode, 0)
            stale = self.preview(world, repo, "link")
            data = json.loads(self.registry(world).read_text())
            identity = proposal["project_id"]
            data["projects"][identity]["aliases"].append("repo://github.com/historical/project")
            self.registry(world).write_text(json.dumps(data))
            before = self.registry(world).read_bytes()
            self.assertEqual(self.apply(world, stale)[0].returncode, 1)
            self.assertEqual(self.registry(world).read_bytes(), before)

    def test_parallel_registrations_preserve_winner_and_replay_is_idempotent(self):
        with World() as world:
            repo = world.repository()
            proposals = [self.preview(world, repo, "create") for _ in range(2)]
            paths = [world.inside(f"p{i}.json") for i in range(2)]
            for path, proposal in zip(paths, proposals):
                path.write_text(json.dumps(proposal))
            children = [world.spawn([sys.executable, "-I", HELPER, "apply", "--proposal", path]) for path in paths]
            results = []
            for child in children:
                stdout, stderr = child.communicate(timeout=20)
                self.assertFalse(stderr)
                results.append((child.returncode, json.loads(stdout)))
            self.assertEqual(sum(code == 0 for code, _ in results), 1, results)
            data = json.loads(self.registry(world).read_text())
            self.assertEqual(len(data["projects"]), 1)
            self.assertEqual(len(data["checkouts"]), 1)
            winner = next(p for p in proposals if p["project_id"] in data["projects"])
            before = self.registry(world).read_bytes()
            result, receipt = self.apply(world, winner)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["already_applied"])
            self.assertEqual(self.registry(world).read_bytes(), before)

    def test_canonical_remote_does_not_merge_different_bank_ids(self):
        with World() as world:
            repo = world.repository()
            world.git("remote", "add", "origin", "https://github.com/owner/project.git", cwd=repo)
            identity, _ = self.create(world, repo)
            independent = world.repository("independent")
            world.git("remote", "add", "origin", "git@github.com:owner/project.git", cwd=independent)
            different = str(uuid.uuid4())
            self.bank(world, independent, different, "repo://github.com/owner/project")
            result, receipt = self.command(world, "preview", "--repo", independent, "--action", "bind", "--reason", "Conflicting bank IDs")
            self.assertEqual(result.returncode, 1, receipt)
            self.assertIn("another project", receipt["error"])
            self.assertEqual(self.status(world, repo)["project_id"], identity)

    def test_copied_pack_spaces_non_git_project_and_unrelated_registry_preserved(self):
        with World() as world:
            pack = world.inside("portable pack")
            shutil.copytree(ROOT / "skills/agent-memory", pack)
            helper = pack / "scripts/project_identity.py"
            world.env["AGENT_MEMORY_CONFIG_HOME"] = str(world.inside("config with spaces"))
            repo = world.inside("non git project")
            repo.mkdir()
            result, proposal = self.command(world, "preview", "--repo", repo, "--action", "create", "--reason", "Non Git fixture", helper=helper)
            self.assertEqual(result.returncode, 0, proposal)
            file = world.inside("p.json")
            file.write_text(json.dumps(proposal))
            result, receipt = self.command(world, "apply", "--proposal", file, helper=helper)
            self.assertEqual(result.returncode, 0, receipt)
            self.bank(world, repo, proposal["project_id"])
            self.assertTrue(self.status(world, repo)["resolved"])
            registry = self.registry(world)
            registry.write_text("Unrelated file; preserve")
            before = registry.read_bytes()
            self.assertEqual(self.command(world, "status", "--repo", repo)[0].returncode, 1)
            self.assertEqual(registry.read_bytes(), before)

    def test_unknown_bank_symlinks_and_busy_state_refuse_without_mutation(self):
        with World() as world:
            repo = world.repository()
            identity, _ = self.create(world, repo)
            router = repo / "memory-bank/MEMORY.md"
            original = router.read_text()
            router.write_text(original.replace('"agent-memory/v1"', '"agent-memory/v999"'))
            self.assertEqual(self.command(world, "status", "--repo", repo)[0].returncode, 1)
            router.write_text(original)
            lock = router.parent / ".agent-memory.lock"
            lock.write_text("Writer")
            self.assertEqual(self.command(world, "status", "--repo", repo)[0].returncode, 1)
            lock.unlink()
            registry = self.registry(world)
            original_registry = registry.read_bytes()
            registry.unlink()
            unrelated = world.inside("unrelated.json")
            unrelated.write_bytes(original_registry)
            registry.symlink_to(unrelated)
            self.assertEqual(self.command(world, "status", "--repo", repo)[0].returncode, 1)
            self.assertTrue(registry.is_symlink())
            self.assertEqual(unrelated.read_bytes(), original_registry)

    def test_project_registry_stale_lock_recovery_is_bounded_and_read_back(self):
        with World() as world:
            repo = world.repository()
            self.create(world, repo)
            lock = self.registry(world).parent / ".projects.lock"
            dead = int(world.run([sys.executable, "-c", "import os; print(os.getpid())"]).stdout)
            owner = {"operation_id": str(uuid.uuid4()), "operation": "project-create", "host": socket.gethostname(),
                     "pid": os.getpid(), "start": "2026-10-22T18:00:00Z"}
            lock.write_text(json.dumps(owner))
            self.assertEqual(self.command(world, "recover")[0].returncode, 1)
            owner["pid"] = dead
            lock.write_text(json.dumps(owner))
            result, receipt = self.command(world, "recover")
            if os.name != "posix":
                self.assertEqual(result.returncode, 1)
                return
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(lock.exists())
            file = world.inside("recovery.json")
            file.write_text(json.dumps(receipt))
            before = self.registry(world).read_bytes()
            result, receipt = self.command(world, "recover", "--proposal", file)
            self.assertEqual(result.returncode, 0, receipt)
            self.assertTrue(receipt["verified"])
            self.assertFalse(lock.exists())
            self.assertEqual(self.registry(world).read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
