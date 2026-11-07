"""Private checked-page exports preserve the public inspection contract."""
import hashlib
import json
import stat
import sys

from world import World
from publication_fixtures import PublicationFixture
from contracts import test_semantic_sharing as semantic_fixtures


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=False, indent=2, allow_nan=False) + '\n').encode()


class CheckedPageExportContract(PublicationFixture):
    def test_empty_inspection_has_only_a_complete_marker(self):
        with World() as w:
            repo, bank, _, _ = self.fixture(w)
            before = self.state(w, repo, bank)
            manifest = w.inside('empty.json'); directory = w.inside('empty-pages')
            receipt = self.good(w, 'reconcile_memory.py', 'inspect-v2', '--repo', repo,
                                '--output', manifest, '--pages-dir', directory)
            view = json.loads(manifest.read_bytes())
            self.assertEqual(view['pages'], [])
            self.assertEqual(receipt['page_count'], 0)
            self.assertEqual({p.name for p in directory.iterdir()}, {'complete.json'})
            marker = json.loads((directory/'complete.json').read_bytes())
            self.assertEqual(marker['page_count'], 0)
            self.assertEqual(before, self.state(w, repo, bank))

    def test_bundle_matches_manifest_and_independent_public_pages(self):
        with World() as w:
            repo, bank, _, _ = self.fixture(w)
            (bank/'activeContext.md').write_text('# Current focus\n\nIndependent checked export evidence.\n')
            before = self.state(w, repo, bank)
            manifest = w.inside('inspection.json'); directory = w.inside('pages')
            receipt = self.good(w, 'reconcile_memory.py', 'inspect-v2', '--repo', repo,
                                '--output', manifest, '--pages-dir', directory)
            raw = manifest.read_bytes(); view = json.loads(raw)
            marker_raw = (directory/'complete.json').read_bytes(); marker = json.loads(marker_raw)
            self.assertEqual(receipt['path'], str(manifest))
            self.assertEqual(receipt['pages_dir'], str(directory))
            self.assertEqual(receipt['sha256'], sha(raw))
            self.assertEqual(receipt['page_export_sha256'], sha(marker_raw))
            self.assertEqual(receipt['page_count'], receipt['review_pages'])
            self.assertEqual(receipt['page_count'], len(view['pages']))
            self.assertEqual(marker, {'schema':'agent-memory/semantic-sharing-page-export-v1',
                'manifest_sha256':view['manifest_sha256'], 'manifest_artifact_sha256':sha(raw),
                'binding_sha256':sha(encoded(view['binding'])), 'page_count':len(view['pages'])})
            self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o700)
            self.assertEqual(stat.S_IMODE(manifest.stat().st_mode), 0o600)
            self.assertEqual(stat.S_IMODE((directory/'complete.json').stat().st_mode), 0o600)
            self.assertEqual({p.name for p in directory.iterdir()},
                             {'complete.json'} | {str(i)+'.json' for i in range(len(view['pages']))})
            bodies = []
            for descriptor in view['pages']:
                page_path = directory/(str(descriptor['index'])+'.json')
                page_raw = page_path.read_bytes()
                self.assertEqual(stat.S_IMODE(page_path.stat().st_mode), 0o600)
                self.assertEqual((len(page_raw), sha(page_raw)), (descriptor['bytes'], descriptor['sha256']))
                page = self.good(w, 'reconcile_memory.py', 'page-v2', '--repo', repo,
                                 '--input', manifest, '--page', str(descriptor['index']))
                self.assertEqual(page_raw, encoded(page))
                bodies.extend(item['source']['body'] for item in page['items'] if 'source' in item)
                self.assertEqual(descriptor['source_keys'], [item['source']['key'] for item in page['items'] if 'source' in item])
                self.assertEqual(descriptor['edges'], [item['edge'] for item in page['items'] if 'edge' in item])
            self.assertTrue(any('Independent checked export evidence.' in body for body in bodies))
            self.assertEqual(before, self.state(w, repo, bank))
    def test_multiple_pages_preserve_every_real_source(self):
        with World() as w:
            repo, bank, _, _ = self.fixture(w)
            helper = semantic_fixtures.SemanticSharingContract()
            paths = []
            for index in range(6):
                body = '# Export subject %d\n\n' % index + ('A durable independent observation. ' * 310) + '\n'
                _, path = helper.add(bank, 'export-subject-%d' % index, body)
                paths.append(path)
            before = self.state(w, repo, bank)
            manifest = w.inside('many.json'); directory = w.inside('many-pages')
            self.good(w, 'reconcile_memory.py', 'inspect-v2', '--repo', repo,
                      '--output', manifest, '--pages-dir', directory)
            view = json.loads(manifest.read_bytes())
            actual_paths = set(); populated_pages = set()
            for descriptor in view['pages']:
                index = descriptor['index']
                raw = (directory/(str(index)+'.json')).read_bytes()
                page = self.good(w, 'reconcile_memory.py', 'page-v2', '--repo', repo,
                                 '--input', manifest, '--page', str(index))
                self.assertEqual(raw, encoded(page))
                self.assertEqual(sha(raw), descriptor['sha256'])
                for item in page['items']:
                    if 'source' in item and item['source']['path'] in paths:
                        row = item['source']
                        if 'A durable independent observation.' in row['body']:
                            actual_paths.add(row['path'])
                            populated_pages.add(index)
            self.assertEqual(actual_paths, set(paths))
            self.assertGreaterEqual(len(populated_pages), 2)
            self.assertEqual(before, self.state(w, repo, bank))


    def test_collisions_and_unsafe_paths_refuse_without_clobbering(self):
        with World() as w:
            repo, bank, _, _ = self.fixture(w)
            before = self.state(w, repo, bank)
            existing = w.inside('existing'); existing.write_bytes(b'keep')
            occupied = w.inside('occupied'); occupied.mkdir(mode=0o700)
            unsafe = w.inside('unsafe'); unsafe.mkdir(mode=0o755); unsafe.chmod(0o755)
            link = w.root/'link'; link.symlink_to(occupied, target_is_directory=True)
            cases = [(existing, w.root/'new-pages'), (w.root/'new.json', occupied),
                     (w.root/'same', w.root/'same'), (w.root/'nested'/'view.json', w.root/'nested'),
                     (w.root/'outside.json', repo/'pages'), (w.root/'unsafe-view.json', unsafe/'pages'),
                     (w.root/'linked.json', link/'pages'), (w.root/'relative.json', 'relative-pages')]
            for output, pages in cases:
                with self.subTest(output=str(output), pages=str(pages)):
                    result, value = self.call(w, 'reconcile_memory.py', 'inspect-v2', '--repo', repo,
                                              '--output', output, '--pages-dir', pages)
                    self.assertEqual(result.returncode, 1, value)
                    self.assertEqual(value['failure_component'], 'output')
                    self.assertEqual(value['failure_code'], 'private-output-invalid')
            self.assertEqual(existing.read_bytes(), b'keep')
            self.assertEqual(list(occupied.iterdir()), [])
            self.assertEqual(before, self.state(w, repo, bank))

    def test_changed_state_or_io_failure_never_completes(self):
        with World() as w:
            repo, bank, _, _ = self.fixture(w)
            source = bank/'activeContext.md'
            source.write_text('# Current focus\n\nChecked export mutation fixture.\n')
            script = '''import os, stat, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from agent_memory import semantic_sharing, sharing_page_export, sharing_holds, recovery_frontier
real = sharing_page_export.write_checked
mode, repo, output, directory, source = sys.argv[2:]
def interrupt(fd, name, raw):
    if name == '0.json' and mode in ('fsync', 'readback'):
        original = os.fsync if mode == 'fsync' else os.read
        def fail(*args):
            if stat.S_ISREG(os.fstat(args[0]).st_mode):
                raise OSError('injected ' + mode + ' failure')
            return original(*args)
        if mode == 'fsync': os.fsync = fail
        else: os.read = fail
        try: real(fd, name, raw)
        finally:
            if mode == 'fsync': os.fsync = original
            else: os.read = original
        return
    real(fd, name, raw)
    if mode == 'source' and name == Path(output).name:
        Path(source).write_text('# Current focus\\n\\nChanged during inspection.\\n')
    if mode == 'hold' and name == Path(output).name:
        original = sharing_holds.load
        def changed(*args, **kwargs):
            return original(*args, **kwargs) + [{'injected': 'changed inventory'}]
        sharing_holds.load = changed
    if mode == 'frontier' and name == Path(output).name:
        original = recovery_frontier.observe
        def changed(*args, **kwargs):
            token = original(*args, **kwargs)
            replacement = '0' * 64 if token['index_sha256'] != '0' * 64 else '1' * 64
            return {**token, 'index_sha256': replacement}
        recovery_frontier.observe = changed
    if mode == 'write' and name == '0.json':
        raise OSError('injected private write failure')
    if mode in ('page', 'fifo') and name == '0.json':
        path = Path(directory)/name
        if mode == 'page': path.write_bytes(path.read_bytes()+b'corrupt')
        else:
            path.unlink()
            os.mkfifo(path, 0o600)
sharing_page_export.write_checked = interrupt
sys.exit(semantic_sharing.main(['inspect-v2','--repo',repo,'--output',output,'--pages-dir',directory]))
'''
            for mode in ('source', 'hold', 'frontier', 'page', 'fifo', 'write', 'fsync', 'readback'):
                with self.subTest(mode=mode):
                    source.write_text('# Current focus\n\nChecked export mutation fixture.\n')
                    output = w.inside(mode+'.json'); directory = w.inside(mode+'-pages')
                    result = w.run([sys.executable, '-I', '-c', script, self.pack/'scripts', mode,
                                    repo, output, directory, source], check=False, timeout=120)
                    self.assertEqual(result.returncode, 1, (result.stdout, result.stderr))
                    self.assertFalse((directory/'complete.json').exists())
