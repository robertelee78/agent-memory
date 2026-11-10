"""Public recovery must preserve typed historical roots and terminal markers."""
import copy
import unittest
import uuid

import layout_recovery_fixtures as f
from contracts import test_comparison
from record_fixtures import project_id
from world import World


def files(bank):
    return {p.relative_to(bank).as_posix(): p.read_bytes()
            for p in bank.rglob('*') if p.is_file()}


def add_tombstone(bank, developer):
    identity = str(uuid.uuid4())
    path = 'history/tombstones/' + identity + '.md'
    metadata = {'id': identity, 'scope': project_id((bank / 'MEMORY.md').read_text()),
                'deleted-at': '2026-10-22T20:00:00Z', 'deletion-authority': developer,
                'reason-class': 'user-request'}
    target = bank / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(test_comparison.render(metadata, ''))
    router = bank / 'MEMORY.md'
    route = '| [' + path + '](<' + path + '>) | Terminal marker | repository | deleted | Before any reuse |\n'
    router.write_text(router.read_text().replace('\n## Omitted contexts', route + '\n## Omitted contexts'))
    return path, metadata


def rebind(request):
    request['review']['unit_map_sha256'] = f.sha(f.encoded(request['unit_map']))
    request['review']['intended_files_sha256'] = f.sha(f.encoded(request['intended_files']))
    request['review']['unit_decisions'] = [
        {'unit_id': unit['unit_id'], 'disposition': unit['disposition'], 'rationale': unit['reason']}
        for unit in request['unit_map']['units']]


def prepare_process(world, command, repo, inspected, request, output):
    return world.run([command, 'bank', 'recover-layout', 'prepare', '--repo', repo,
                      '--operation', inspected['operation_id'], '--input', f.save(world, request),
                      '--output', output], check=False, timeout=60)


class RecoveryRootSemanticsContract(unittest.TestCase):
    def test_historical_source_and_tombstone_survive_repair_with_exact_originals(self):
        with World() as world:
            command, source = f.installed(world)
            repo, bank, native = f.memory(world, oversized=True)
            tomb, marker = add_tombstone(bank, str(uuid.uuid4()))
            native[tomb] = (bank / tomb).read_text()
            native['MEMORY.md'] = native['MEMORY.md'].replace(
                '\n## Omitted contexts', '| [' + tomb + '](<' + tomb + '>) | Terminal marker | repository | deleted | Before any reuse |\n\n## Omitted contexts')
            original = files(bank)
            history = next(path for path in native if path.startswith('history/sources/'))
            old_metadata, old_body = test_comparison.unpack(native[history])
            _, inspected = f.inspect(world, command, repo)
            proposal, _ = f.prepare(world, command, source, repo, inspected, native)
            applied = f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertTrue(applied['semantic_recovery_complete'])
            self.assertEqual((bank / history).read_text(), native[history])
            self.assertEqual(test_comparison.unpack((bank / history).read_text()), (old_metadata, old_body))
            self.assertEqual(test_comparison.unpack((bank / tomb).read_text()), (marker, ''))
            directory = f.private_directory(world, bank, inspected['operation_id'])
            for item in inspected['manifest']['sources']:
                self.assertEqual((directory / item['blob']).read_bytes(), original[item['path']])

    def test_historical_provenance_cannot_be_rewritten_as_layout_repair(self):
        with World() as world:
            command, source = f.installed(world)
            repo, bank, native = f.memory(world, oversized=True)
            _, inspected = f.inspect(world, command, repo)
            control = f.review_request(world, command, source, repo, inspected, native)
            rebind(control)
            accepted = prepare_process(world, command, repo, inspected, control, world.inside('original-history-proposal.json'))
            self.assertEqual(accepted.returncode, 0, accepted.stdout + accepted.stderr)
            path = next(p for p in native if p.startswith('history/sources/'))
            metadata, body = test_comparison.unpack(native[path])
            metadata = copy.deepcopy(metadata)
            metadata['origin']['session'] = 'different-unverified-session'
            intended = dict(native)
            intended[path] = test_comparison.render(metadata, body)
            request = copy.deepcopy(control)
            request['intended_files'] = intended
            unit = next(u for u in request['unit_map']['units'] if u['targets'] and u['targets'][0]['path'] == path)
            unit['targets'][0]['sha256'] = f.sha(intended[path].encode())
            rebind(request)
            before = files(bank)
            index = (repo / '.git/index').read_bytes()
            output = world.inside('rewritten-history-proposal.json')
            result = prepare_process(world, command, repo, inspected, request, output)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())
            self.assertEqual(files(bank), before)
            self.assertEqual((repo / '.git/index').read_bytes(), index)

    def test_terminal_marker_cannot_be_discarded_because_backup_exists(self):
        with World() as world:
            command, source = f.installed(world)
            repo, bank, native = f.memory(world, oversized=True)
            tomb, _ = add_tombstone(bank, str(uuid.uuid4()))
            native[tomb] = (bank / tomb).read_text()
            native['MEMORY.md'] = native['MEMORY.md'].replace(
                '\n## Omitted contexts', '| [' + tomb + '](<' + tomb + '>) | Terminal marker | repository | deleted | Before any reuse |\n\n## Omitted contexts')
            _, inspected = f.inspect(world, command, repo)
            original = next(row for row in inspected['manifest']['sources'] if row['path'] == tomb)
            directory = f.private_directory(world, bank, inspected['operation_id'])
            self.assertEqual((directory / original['blob']).read_bytes(), (bank / tomb).read_bytes())
            request = f.review_request(world, command, source, repo, inspected, native)
            unit = next(u for u in request['unit_map']['units'] if u['targets'] and u['targets'][0]['path'] == tomb)
            unit.update(semantic_kind='non-guidance', disposition='non-guidance-discard', targets=[],
                        reason='A private backup exists.')
            del request['intended_files'][tomb]
            del request['metadata_plan']['files'][tomb]
            request['intended_files']['MEMORY.md'] = ''.join(
                line for line in request['intended_files']['MEMORY.md'].splitlines(keepends=True)
                if '[' + tomb + ']' not in line)
            router_unit = next(u for u in request['unit_map']['units'] if u['targets'] and u['targets'][0]['path'] == 'MEMORY.md')
            router_unit['targets'][0]['sha256'] = f.sha(request['intended_files']['MEMORY.md'].encode())
            rebind(request)
            decision = next(d for d in request['review']['unit_decisions'] if d['unit_id'] == unit['unit_id'])
            decision['authority'] = {**request['author'], 'statement': 'Explicitly authorize this source-bound non-guidance archive/removal; byte retention alone is not the reason.'}
            request['review']['metadata_plan_sha256'] = f.sha(f.encoded(request['metadata_plan']))
            before = files(bank)
            output = world.inside('discarded-marker-proposal.json')
            result = prepare_process(world, command, repo, inspected, request, output)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())
            self.assertEqual(files(bank), before)
            self.assertEqual((directory / original['blob']).read_bytes(), before[tomb])
