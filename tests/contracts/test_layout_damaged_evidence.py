"""Interrupted recovery must refuse missing or changed original evidence and review."""
import stat
import unittest

from world import World
from contracts import test_layout_recovery as existing
import layout_recovery_fixtures as recovery


class LayoutDamagedEvidence(unittest.TestCase):
    def journey(self, evidence):
        helper = existing.LayoutRecoveryContract()
        with World() as world:
            command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, prepared = recovery.prepare(world, command, source, repo, inspected, native)
            recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                          env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-after-marker'})
            directory = recovery.private_directory(world, bank, safe['operation_id'])
            path = (directory / inspected['manifest']['sources'][0]['blob'] if evidence == 'original'
                    else directory / 'reviews' / (prepared['proposal']['review_id'] + '.json'))
            original = path.read_bytes()
            original_mode = stat.S_IMODE(path.stat().st_mode)
            canonical = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            for variant in ('changed', 'missing'):
                with self.subTest(evidence=evidence, variant=variant):
                    if variant == 'changed':
                        path.write_bytes(original + b'changed')
                    else:
                        path.unlink()
                    recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                                  '--mode', 'finish', '--output', world.inside('refused-' + variant + '.json'), expected=1)
                    self.assertEqual(path.read_bytes() if path.exists() else None,
                                     original + b'changed' if variant == 'changed' else None)
                    self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes()
                                      for p in bank.rglob('*') if p.is_file()}, canonical)
                    helper.assert_fault_work_unchanged(world, repo, git)
                    path.write_bytes(original); path.chmod(original_mode)
            preview = world.inside('evidence-restored.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                          '--mode', 'finish', '--output', preview)
            outcome = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', preview)
            self.assertTrue(outcome['verified'])
            self.assertFalse(outcome['semantic_recovery_complete'])
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*')
                              if p.is_file() and p.name not in ('.agent-memory-transaction.md', '.agent-memory-recovery.md')},
                             {path: body.encode() for path, body in native.items()})
            helper.assert_fault_work_unchanged(world, repo, git)

    def test_original_evidence_refuses_changed_or_missing(self):
        self.journey('original')

    def test_review_refuses_changed_or_missing(self):
        self.journey('review')
