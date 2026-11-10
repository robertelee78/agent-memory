"""Public recovery controls using naturally generated macOS provenance."""
import ctypes
import os
import stat
import sys
import unittest

from world import World
import layout_recovery_fixtures as fixture


PROVENANCE = 'com.apple.provenance'


def native_xattrs():
    libc = ctypes.CDLL(None, use_errno=True)
    libc.listxattr.argtypes = [ctypes.c_char_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int]
    libc.listxattr.restype = ctypes.c_ssize_t
    libc.getxattr.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_void_p,
                             ctypes.c_size_t, ctypes.c_uint32, ctypes.c_int]
    libc.getxattr.restype = ctypes.c_ssize_t
    libc.setxattr.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_void_p,
                             ctypes.c_size_t, ctypes.c_uint32, ctypes.c_int]
    libc.setxattr.restype = ctypes.c_int
    return libc


def checked(result):
    if result < 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error))
    return result


def xattrs(path):
    libc = native_xattrs()
    encoded_path = os.fsencode(path)
    size = checked(libc.listxattr(encoded_path, None, 0, 0))
    names = ctypes.create_string_buffer(size)
    if size and checked(libc.listxattr(encoded_path, names, size, 0)) != size:
        raise RuntimeError('xattr names changed while reading')
    result = {}
    for name in names.raw.split(b'\0')[:-1] if size else []:
        count = checked(libc.getxattr(encoded_path, name, None, 0, 0, 0))
        value = ctypes.create_string_buffer(count)
        if checked(libc.getxattr(encoded_path, name, value, count, 0, 0)) != count:
            raise RuntimeError('xattr value changed while reading')
        result[os.fsdecode(name)] = value.raw.hex()
    return result


def set_xattr(path, name, value):
    libc = native_xattrs()
    buffer = ctypes.create_string_buffer(value, len(value))
    checked(libc.setxattr(os.fsencode(path), os.fsencode(name), buffer, len(value), 0, 0))


def snapshot(bank, paths):
    return {path: ((bank / path).read_bytes(), stat.S_IMODE((bank / path).stat().st_mode),
                   xattrs(bank / path)) for path in paths}


@unittest.skipUnless(sys.platform == 'darwin', 'macOS provenance is Darwin-specific')
class DarwinProvenanceRecoveryContract(unittest.TestCase):
    def setup_repair(self, world):
        command, source = fixture.installed(world)
        repo, bank, native = fixture.memory(world, oversized=True)
        active = bank / 'activeContext.md'
        if PROVENANCE not in xattrs(active):
            self.skipTest('filesystem did not generate macOS provenance on fixture file')
        originals = snapshot(bank, native)
        return command, source, repo, bank, native, originals

    def test_apply_and_child_rollback_preserve_exact_per_path_xattrs(self):
        with World() as world:
            command, source, repo, bank, native, originals = self.setup_repair(world)
            safe, inspected = fixture.inspect(world, command, repo)
            rows = {row['path']: row for row in inspected['manifest']['physical_entries']}
            for path, (_, _, attrs) in originals.items():
                self.assertEqual(rows[path]['xattrs'], attrs)
            proposal, prepared = fixture.prepare(world, command, source, repo, inspected, native)
            plan = prepared['proposal']['metadata_plan']['files']
            for path, (_, _, attrs) in originals.items():
                self.assertEqual(plan[path]['xattrs'], attrs)
            result = fixture.call(world, command, 'apply', '--repo', repo,
                                  '--operation', safe['operation_id'], '--proposal', proposal)
            self.assertTrue(result['canonical_eligibility'])
            for path, (_, mode, attrs) in originals.items():
                self.assertEqual((bank / path).read_text(), native[path])
                self.assertEqual(stat.S_IMODE((bank / path).stat().st_mode), mode)
                self.assertEqual(xattrs(bank / path), attrs)
            restore = world.inside('provenance-child-restore.json')
            fixture.call(world, command, 'rollback', '--repo', repo,
                         '--operation', safe['operation_id'], '--output', restore)
            fixture.call(world, command, 'rollback', '--repo', repo, '--proposal', restore)
            self.assertEqual(snapshot(bank, originals), originals)

    def test_changed_metadata_refuses_before_replacement(self):
        with World() as world:
            command, source, repo, bank, native, originals = self.setup_repair(world)
            _, inspected = fixture.inspect(world, command, repo)
            proposal, _ = fixture.prepare(world, command, source, repo, inspected, native)
            target = bank / 'activeContext.md'
            set_xattr(target, 'org.agent-memory.drift-control', b'controlled drift')
            before = snapshot(bank, native)
            fixture.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1)
            self.assertEqual(snapshot(bank, native), before)
            self.assertEqual(xattrs(target)[PROVENANCE], originals['activeContext.md'][2][PROVENANCE])

    def test_reviewed_wrong_provenance_refuses_without_replacement(self):
        with World() as world:
            command, source, repo, bank, native, originals = self.setup_repair(world)
            _, inspected = fixture.inspect(world, command, repo)
            request = fixture.review_request(world, command, source, repo, inspected, native)
            attrs = request['metadata_plan']['files']['activeContext.md']['xattrs']
            value = bytes.fromhex(attrs[PROVENANCE])
            attrs[PROVENANCE] = (value + b'\x00').hex()
            request['review']['metadata_plan_sha256'] = fixture.sha(fixture.encoded(request['metadata_plan']))
            fixture.call(world, command, 'prepare', '--repo', repo, '--operation', inspected['operation_id'],
                         '--input', fixture.save(world, request), '--output', world.inside('wrong-metadata.json'),
                         expected=1)
            self.assertEqual(snapshot(bank, native), originals)

    def test_unknown_xattr_refuses_inspection_without_mutation(self):
        with World() as world:
            command, _, repo, bank, native, _ = self.setup_repair(world)
            target = bank / 'activeContext.md'
            set_xattr(target, 'org.agent-memory.contract-control', b'controlled')
            before = snapshot(bank, native)
            fixture.call(world, command, 'inspect', '--repo', repo,
                         '--output', world.inside('unknown-xattr.json'), expected=1)
            self.assertEqual(snapshot(bank, native), before)
            self.assertEqual(bytes.fromhex(xattrs(target)['org.agent-memory.contract-control']), b'controlled')
