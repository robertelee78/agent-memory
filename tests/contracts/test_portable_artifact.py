"""A complete copied package identifies itself and operates without clients."""
import ast
import hashlib
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World

ROOT = Path(__file__).resolve().parents[2]
PACK = ROOT / 'skills/agent-memory'


def observed(root):
    return {p.relative_to(root).as_posix():
            (hashlib.sha256(p.read_bytes()).hexdigest(), p.stat().st_mode & 0o777)
            for p in root.rglob('*') if p.is_file()}


class PortableArtifactContract(unittest.TestCase):
    def copy(self, world):
        pack = world.inside('copied complete artifact')
        shutil.copytree(PACK, pack)
        for name in ('codex', 'claude', 'opencode', 'cache', 'credentials'):
            shutil.rmtree(world.paths[name])
        return pack

    def call(self, world, pack, helper, *args, check=True):
        result = world.run([sys.executable, '-I', pack/'scripts'/helper, *args], check=False)
        value = json.loads(result.stdout)
        if check:
            self.assertEqual(result.returncode, 0, value or result.stderr)
        return result, value

    def save(self, world, value, name):
        path = world.inside(name); path.write_text(json.dumps(value)); return path

    def inspect(self, world, pack):
        return self.call(world, pack, 'artifact.py', 'inspect', '--pack', pack)[1]

    def test_complete_copy_outside_git_has_the_same_identity_and_explicit_optional_prerequisites(self):
        with World() as world:
            pack = self.copy(world); before = observed(world.root)
            source = self.inspect(world, PACK); copied = self.inspect(world, pack)
            self.assertEqual(source['artifact_digest'], copied['artifact_digest'])
            self.assertEqual(source['files'], copied['files'])
            self.assertEqual(copied['file_count'], len(observed(pack)))
            self.assertEqual(copied['compatibility']['readable_bank_schemas'], ['agent-memory/v1'])
            self.assertEqual(copied['compatibility']['migrator_source_schemas'], [])
            self.assertEqual(copied['compatibility']['required_agent_clients'], [])
            self.assertEqual(copied['compatibility']['required_python_packages'], [])
            self.assertEqual(copied['optional_adapters'][0]['commands'], ['gh'])
            self.assertTrue(copied['optional_adapters'][0]['requires_authentication'])
            for field in ('writes_performed', 'candidate_code_executed', 'source_origin_authenticated',
                          'installation_state_checked', 'runtime_behavior_certified'):
                self.assertFalse(copied[field])
            self.assertEqual(observed(world.root), before)

    def test_every_implemented_documented_helper_imports_and_dispatches_from_the_copy(self):
        with World() as world:
            pack = self.copy(world); value = self.inspect(world, pack)
            forms = {(m['script'], m['operation']) for m in value['documented_commands'] if 'script' in m}
            self.assertGreaterEqual(len(forms), 66)
            before = observed(world.root)
            for script, operation in sorted(forms, key=str):
                with self.subTest(script=script, operation=operation):
                    result = world.run([sys.executable, '-I', pack/script,
                                        *([operation] if operation else []), '--help'], check=False)
                    self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(observed(world.root), before)

    def test_core_python_imports_require_only_the_standard_library_and_bundled_package(self):
        allowed = set(sys.stdlib_module_names) | {'agent_memory'}
        with World() as world:
            pack = self.copy(world)
            for path in (pack/'scripts').rglob('*.py'):
                with self.subTest(path=path.relative_to(pack)):
                    tree = ast.parse(path.read_text())
                    for node in ast.walk(tree):
                        if isinstance(node, ast.Import):
                            self.assertTrue({name.name.split('.')[0] for name in node.names} <= allowed)
                        elif isinstance(node, ast.ImportFrom) and not node.level:
                            self.assertIn(node.module.split('.')[0], allowed)

    def test_copied_helpers_initialize_checkpoint_and_recall_while_preserving_unfinished_code(self):
        with World() as world:
            pack = self.copy(world); repo = world.repository()
            (repo/'README.md').write_text('# Retry worker\n\nInvestigate retry behavior before generalizing it.\n')
            (repo/'app.py').write_text('RETRY_LIMIT = 3\n')
            world.git('add', '.', cwd=repo); world.git('commit', '-m', 'Actual disposable source', cwd=repo)
            def run(helper, *args):
                return self.call(world, pack, helper, *args)[1]
            def apply(helper, proposal, name):
                return run(helper, 'apply', '--proposal', self.save(world, proposal, name))
            proposal = run('developer_identity.py', 'preview', '--create', '--label', 'Portable-copy developer')
            self.assertTrue(apply('developer_identity.py', proposal, 'developer.json')['verified'])
            proposal = run('scope_binding.py', 'preview', '--repo', repo, '--action', 'create',
                           '--reason', 'Explicit isolated project setup')
            identity = apply('scope_binding.py', proposal, 'project.json')
            project = identity['project_id']
            (repo/'app.py').write_text('RETRY_LIMIT = 4  # unfinished local change\n')
            world.git('add', 'app.py', cwd=repo)
            (repo/'scratch.txt').write_text('Preserve this unrelated note.\n')
            before = ((repo/'app.py').read_bytes(), (repo/'scratch.txt').read_bytes(),
                      (repo/'.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            inventory = run('initialize.py', 'inspect', '--repo', repo)
            review = {'inventory': inventory, 'existing_memory_reviewed': False, 'claims': [], 'questions': []}
            proposal = run('initialize.py', 'preview', '--repo', repo,
                           '--review', self.save(world, review, 'initialization-review.json'))
            self.assertTrue(apply('initialize.py', proposal, 'initialize.json')['verified'])
            bank = repo/'memory-bank'
            current = run('checkpoint.py', 'inspect', '--repo', repo)
            self.assertFalse(current['preparation_required'])
            request = {'focus': 'Review the retry component from this copied artifact.',
                       'recent_changes': ['Inspected the actual local source map; feature code is still unfinished.'],
                       'open_questions': ['Does the retry lesson apply beyond this worker?'],
                       'next_step': 'Run an isolated retry probe before admitting a durable lesson.',
                       'evidence': ['../README.md'], 'conditions': {'branch': 'main'},
                       'harness': 'custom', 'session': 'portable-copy',
                       'observed_active_sha256': current['observed_active_sha256']}
            prior_bank = {p.name: p.read_bytes() for p in bank.iterdir() if p.is_file()}
            proposal = run('checkpoint.py', 'preview', '--repo', repo,
                           '--input', self.save(world, request, 'checkpoint-input.json'))
            receipt = apply('checkpoint.py', proposal, 'checkpoint.json')
            self.assertTrue(receipt['verified']); self.assertFalse(receipt['publication_verified'])
            self.assertEqual(receipt['changed_memory_paths'], ['memory-bank/activeContext.md'])
            for name, raw in prior_bank.items():
                if name != 'activeContext.md':
                    self.assertEqual((bank/name).read_bytes(), raw)
            query = {'project_id': project, 'query': '', 'ids': [], 'paths': ['activeContext.md'],
                     'context': {'branch': 'main'}, 'kinds': [], 'minimum_trust': 'reviewed',
                     'limit': 4, 'include_context': True}
            recall = run('retrieve.py', '--repo', repo, '--request', self.save(world, query, 'recall.json'))
            self.assertEqual([r['path'] for r in recall['results']], ['activeContext.md'])
            self.assertIn('Review the retry component', recall['results'][0]['body'])
            self.assertFalse(recall['publication_verified'])
            self.assertEqual(before, ((repo/'app.py').read_bytes(), (repo/'scratch.txt').read_bytes(),
                             (repo/'.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout))
            for name in ('codex', 'claude', 'opencode', 'cache', 'credentials'):
                self.assertFalse(world.paths[name].exists())

    def test_missing_or_false_runtime_declarations_are_refused_without_modifying_the_copy(self):
        for mutation in ('missing-compatibility', 'missing-adapter', 'requires-client', 'omits-native-schema'):
            with self.subTest(mutation=mutation), World() as world:
                pack = self.copy(world); path = pack/'schemas/runtime-compatibility.json'
                if mutation == 'missing-compatibility':path.unlink()
                elif mutation == 'missing-adapter':(pack/'adapters/github-publication.json').unlink()
                else:
                    spec = json.loads(path.read_text())
                    spec['required_agent_clients' if mutation == 'requires-client' else 'readable_bank_schemas'] = ['unsupported']
                    path.write_text(json.dumps(spec))
                before = observed(pack)
                result, value = self.call(world, pack, 'artifact.py', 'inspect', '--pack', pack, check=False)
                self.assertEqual(result.returncode, 1, value); self.assertFalse(value['valid'])
                self.assertFalse(value['writes_performed']); self.assertEqual(observed(pack), before)

    def test_inventory_digest_changes_for_actual_bytes_or_modes_without_claiming_a_signature(self):
        with World() as world:
            pack = self.copy(world); original = self.inspect(world, pack)
            path = pack/'maintain/artifact/GUIDE.md'; path.write_text(path.read_text()+'\nAdditional maintainer context.\n')
            changed = self.inspect(world, pack)
            self.assertNotEqual(changed['artifact_digest'], original['artifact_digest'])
            helper = pack/'scripts/artifact.py'; helper.chmod(0o600)
            mode = self.inspect(world, pack)
            self.assertNotEqual(mode['artifact_digest'], changed['artifact_digest'])
            self.assertEqual(mode['files']['scripts/artifact.py']['sha256'], changed['files']['scripts/artifact.py']['sha256'])
            self.assertFalse(mode['source_origin_authenticated'])


if __name__ == '__main__':
    unittest.main()
