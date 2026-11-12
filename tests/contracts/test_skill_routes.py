"""Public pack checks use copied artifacts, damaged fixtures and real dispatch."""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World

ROOT = Path(__file__).resolve().parents[2]
PACK = ROOT / 'skills/agent-memory'
CLI = PACK / 'scripts/validate_pack.py'


def observed(root):
    return {p.relative_to(root).as_posix():
            (hashlib.sha256(p.read_bytes()).hexdigest(), p.stat().st_mode & 0o777)
            for p in root.rglob('*') if p.is_file() and not p.is_symlink()}


class SkillRouteContract(unittest.TestCase):
    def copy(self, world):
        path = world.inside('portable package with spaces')
        shutil.copytree(PACK, path)
        return path

    def check(self, world, pack):
        result = world.run([sys.executable, '-I', CLI, '--pack', pack], check=False)
        value = json.loads(result.stdout)
        self.assertEqual(result.returncode, 0 if value['valid'] else 1, result.stderr)
        self.assertFalse(value['writes_performed'])
        self.assertFalse(value['candidate_code_executed'])
        self.assertFalse(value['semantic_completeness_verified'])
        return value

    def test_complete_copied_pack_validates_without_changing_pack_or_caller_state(self):
        with World() as world:
            pack = self.copy(world)
            before = observed(world.root)
            value = self.check(world, pack)
            self.assertTrue(value['valid'], value)
            self.assertGreaterEqual(len(value['guides']), 35)
            self.assertEqual(value['file_operations']['offline/GUIDE.md'],
                             ['initialize', 'inspect', 'checkpoint', 'resume'])
            self.assertEqual(set(value['files']), set(observed(pack)))
            self.assertEqual(observed(world.root), before)
            for path, (digest, mode) in observed(pack).items():
                self.assertEqual(value['files'][path], {'sha256': digest, 'mode': oct(mode)})

    def test_every_documented_helper_dispatch_also_accepts_real_public_help(self):
        with World() as world:
            pack = self.copy(world); value = self.check(world, pack)
            self.assertTrue(value['valid'], value)
            forms = {(m['script'], m['operation']) for m in value['documented_commands'] if 'script' in m}
            self.assertGreaterEqual(len(forms), 65)
            before = observed(world.root)
            for script, operation in sorted(forms, key=str):
                with self.subTest(script=script, operation=operation):
                    result = world.run([sys.executable, '-I', pack / script,
                                        *([operation] if operation else []), '--help'], check=False)
                    self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(observed(world.root), before)

    def test_malformed_routes_links_frontmatter_and_leaf_contracts_are_refused(self):
        cases = {
            'duplicate-skill': ('registration', lambda p: (p/'extra').mkdir() or
                                shutil.copy2(p/'SKILL.md', p/'extra/SKILL.md')),
            'missing-link': ('link', lambda p: self.append(p/'SKILL.md', '\n[missing](missing.md)\n')),
            'escape-link': ('link', lambda p: self.append(p/'SKILL.md', '\n[escape](../outside.md)\n')),
            'missing-anchor': ('anchor', lambda p: self.append(p/'SKILL.md', '\n[bad](recall/GUIDE.md#missing-anchor)\n')),
            'undefined-reference': ('link', lambda p: self.append(p/'SKILL.md', '\n[bad][absent]\n')),
            'orphan': ('reachability', lambda p: (p/'orphan.md').write_text('# Unreachable\n')),
            'unused-reference-is-not-route': ('reachability', lambda p:
                (p/'orphan.md').write_text('# Unreachable\n') and self.append(p/'SKILL.md', '\n[unused]: orphan.md\n')),
            'version': ('frontmatter', lambda p: self.replace_version(p, 'broken-version')),
            'duplicate-metadata': ('frontmatter', lambda p: self.replace(p/'SKILL.md', 'name: agent-memory', 'name: agent-memory\nname: agent-memory')),
            'empty-prerelease': ('frontmatter', lambda p: self.replace_version(p, '0.0.1-..dev')),
            'leading-zero-prerelease': ('frontmatter', lambda p: self.replace_version(p, '0.0.1-01')),
            'unicode-core-version': ('frontmatter', lambda p: self.replace_version(p, '1\u0662.0.0')),
            'missing-section': ('leaf-contract', lambda p: self.replace(p/'checkpoint/GUIDE.md', '## Observe', '## Missing')),
            'duplicate-section': ('leaf-contract', lambda p: self.append(p/'checkpoint/GUIDE.md', '\n## Observe\nAgain.\n')),
            'wrong-order': ('leaf-contract', lambda p: self.replace(p/'checkpoint/GUIDE.md', '## Observe', '## Verify')),
            'wrong-domain': ('domain', lambda p: self.replace(p/'checkpoint/GUIDE.md', 'domain: capture', 'domain: unknown')),
            'negative-owner-binding': ('domain', lambda p: self.replace(p/'checkpoint/GUIDE.md',
                'MUST first load and follow the [capture domain]', 'MUST NOT load or follow the [capture domain]')),
            'optional-owner-binding': ('domain', lambda p: self.replace(p/'checkpoint/GUIDE.md',
                'MUST first load and follow the [capture domain]', 'MAY load and follow the [capture domain]')),
            'cross-wired-root-route': ('route', lambda p: self.replace(p/'SKILL.md',
                '| Checkpoint active context | [capture](capture/GUIDE.md)', '| Checkpoint active context | [access](access/GUIDE.md)')),
            'missing-asset': ('link', lambda p: (p/'templates/router-markdown.example.txt').unlink()),
            'unlinked-schema': ('asset', lambda p: self.append(p/'SKILL.md', '\nThe schema is `schemas/missing.json`.\n')),
            'missing-helper': ('dispatch', lambda p: (p/'scripts/checkpoint.py').unlink()),
            'unknown-command': ('dispatch', lambda p: self.replace(p/'checkpoint/GUIDE.md', 'checkpoint.py" inspect', 'checkpoint.py" nonexistent')),
            'unknown-inline-command': ('dispatch', lambda p: self.append(p/'checkpoint/GUIDE.md', '\nUse `python3 -I "$SKILL_DIR/scripts/checkpoint.py" absent`.\n')),
            'config-remote-is-not-config-operation': ('dispatch', lambda p: self.append(p/'SKILL.md', '\n```sh\ngit push config HEAD:main\n```\n')),
            'git-followed-by-extra-operation': ('dispatch', lambda p: self.append(p/'SKILL.md', '\n```sh\ngit config example.value test && git push config HEAD:main\n```\n')),
            'git-pipe-and-extra-operation': ('dispatch', lambda p: self.append(p/'SKILL.md', '\n```sh\ngit config example.value test |& git push config HEAD:main\n```\n')),
            'git-command-substitution': ('dispatch', lambda p: self.append(p/'SKILL.md', '\n```sh\ngit config example.value "$(git push config HEAD:main)"\n```\n')),
            'git-expanded-global-value': ('dispatch', lambda p: self.append(p/'SKILL.md', '\n```sh\ngit -C $EXTRA_ARGS config example.value test\n```\n')),
            'git-globbed-global-value': ('dispatch', lambda p: self.append(p/'SKILL.md', '\n```sh\ngit -C * config example.value test\n```\n')),
            'helper-followed-by-extra-operation': ('dispatch', lambda p: self.append(p/'SKILL.md', '\n```sh\npython3 -I "$SKILL_DIR/scripts/checkpoint.py" inspect --repo /tmp/example; git push config HEAD:main\n```\n')),
            'unknown-option': ('dispatch', lambda p: self.replace(p/'checkpoint/GUIDE.md', 'inspect --repo', 'inspect --imaginary')),
            'lowercase-requirement': ('rfc-form', lambda p: self.append(p/'SKILL.md', '\nA session must preserve evidence.\n')),
            'mixed-negative': ('rfc-form', lambda p: self.append(p/'SKILL.md', '\nA session MUST not lose evidence.\n')),
            'compound-requirement': ('rfc-form', lambda p: self.append(p/'SKILL.md', '\nA session MUST MAY lose evidence.\n')),
            'whole-tree-loading': ('loading', lambda p: self.append(p/'SKILL.md', '\nA session MUST read every guide.\n')),
            'client-coupling': ('client-neutrality', lambda p: self.append(p/'SKILL.md', '\nA session MUST require Codex for recall.\n')),
            'executable-mode': ('asset-mode', lambda p: self.script_without_execute(p)),
        }
        for name, (code, mutate) in cases.items():
            with self.subTest(name=name), World() as world:
                pack = self.copy(world); mutate(pack); before = observed(pack)
                value = self.check(world, pack)
                self.assertFalse(value['valid'], name)
                self.assertEqual(value['issues'][0]['code'], code, value)
                self.assertEqual(observed(pack), before)

    def test_mandatory_loading_cycles_fail_but_conditional_navigation_remains_valid(self):
        with World() as world:
            pack = self.copy(world)
            self.append(pack/'capture/GUIDE.md', '\nA session MUST read [checkpoint](../checkpoint/GUIDE.md) when checkpointing.\n')
            self.append(pack/'capture/GUIDE.md', '\nFor qualified duplicate records, a session MUST read [checkpoint](../checkpoint/GUIDE.md).\n')
            self.assertTrue(self.check(world, pack)['valid'])
            self.append(pack/'capture/GUIDE.md', '\nA session MUST read [checkpoint](../checkpoint/GUIDE.md).\n')
            value = self.check(world, pack)
            self.assertFalse(value['valid']); self.assertEqual(value['issues'][0]['code'], 'loading-cycle')

    def test_fenced_project_examples_are_data_and_real_reference_and_duplicate_heading_anchors_work(self):
        with World() as world:
            pack = self.copy(world)
            self.append(pack/'SKILL.md', '\n````markdown\n```\n[project-only](../absent.md)\n````\n'
                        '\n### Repeated\n\n### Repeated\n\n[second](#repeated-1)\n'
                        '\n[the recall domain][recall-alias]\n\n[recall-alias]: recall/GUIDE.md\n')
            self.assertTrue(self.check(world, pack)['valid'])
            self.append(pack/'SKILL.md', '\n[real broken](#absent)\n')
            value = self.check(world, pack)
            self.assertFalse(value['valid']); self.assertEqual(value['issues'][0]['code'], 'anchor')

    def test_purpose_clauses_cannot_hide_an_unconditional_loading_cycle(self):
        # Actual independently reproduced false green on c796dfa, not a
        # hypothetical English-parser test. #15 rejection and source binding:
        # https://github.com/robertelee78/agent-memory/issues/15#issuecomment-6373996736
        for purpose in (' for completeness', ' for complete context', '\nfor completeness',
                        ' to complete this operation'):
            with self.subTest(purpose=purpose), World() as world:
                pack = self.copy(world)
                self.append(pack/'capture/GUIDE.md',
                            '\nA session MUST read [checkpoint](../checkpoint/GUIDE.md)' + purpose + '.\n')
                value = self.check(world, pack)
                self.assertFalse(value['valid'], purpose)
                self.assertEqual(value['issues'][0]['code'], 'loading-cycle')

    def test_internal_symlink_is_refused_without_reading_its_target(self):
        with World() as world:
            pack = self.copy(world); target = world.inside('outside.txt'); target.write_text('private fixture')
            (pack/'linked.txt').symlink_to(target)
            value = self.check(world, pack)
            self.assertFalse(value['valid']); self.assertEqual(value['issues'][0]['code'], 'file-type')
            self.assertEqual(target.read_text(), 'private fixture')

    def test_unreadable_directory_cannot_hide_a_second_registered_skill(self):
        with World() as world:
            pack = self.copy(world); hidden = pack/'hidden-directory'; hidden.mkdir()
            shutil.copy2(pack/'SKILL.md', hidden/'SKILL.md')
            try:
                hidden.chmod(0o000)
                value = self.check(world, pack)
                self.assertFalse(value['valid'])
                self.assertEqual(value['issues'][0]['code'], 'asset-mode')
                self.assertEqual(hidden.stat().st_mode & 0o777, 0)
                hidden.chmod(0o700)
                value = self.check(world, pack)
                self.assertFalse(value['valid'])
                self.assertEqual(value['issues'][0]['code'], 'registration')
            finally:
                hidden.chmod(0o700)

    def test_modifiers_and_purpose_words_cannot_hide_required_load_cycles(self):
        # Source-bound independent preflight reproductions on 89b5fa46 (#15).
        directives = (
            'A session MUST always read [checkpoint](../checkpoint/GUIDE.md).',
            'A session MUST preload [checkpoint](../checkpoint/GUIDE.md) for completeness.',
            'A session MUST read [checkpoint](../checkpoint/GUIDE.md) for complete context only.',
            'A session MUST read [checkpoint](../checkpoint/GUIDE.md) at all times.',
        )
        for directive in directives:
            with self.subTest(directive=directive), World() as world:
                pack = self.copy(world)
                self.append(pack/'capture/GUIDE.md', '\n' + directive + '\n')
                value = self.check(world, pack)
                self.assertFalse(value['valid'], directive)
                self.assertEqual(value['issues'][0]['code'], 'loading-cycle')

    def test_supported_reference_and_html_links_cannot_hide_a_required_load_cycle(self):
        directives = (
            'A session MUST read [checkpoint][cp].\n\n[cp]: ../checkpoint/GUIDE.md',
            'A session MUST read [checkpoint][].\n\n[checkpoint]: ../checkpoint/GUIDE.md',
            'A session MUST read [checkpoint].\n\n[checkpoint]: ../checkpoint/GUIDE.md',
            'A session MUST read <a href="../checkpoint/GUIDE.md">checkpoint</a>.',
        )
        for directive in directives:
            with self.subTest(directive=directive), World() as world:
                pack = self.copy(world)
                self.append(pack/'capture/GUIDE.md', '\n' + directive + '\n')
                value = self.check(world, pack)
                self.assertFalse(value['valid'], directive)
                self.assertEqual(value['issues'][0]['code'], 'loading-cycle')

    def test_coordinated_objects_and_later_conditions_preserve_the_initial_required_load(self):
        directives = (
            'A session MUST read [bank](../bank/GUIDE.md) and [checkpoint](../checkpoint/GUIDE.md).',
            'A session MUST read [checkpoint](../checkpoint/GUIDE.md) and [bank](../bank/GUIDE.md).',
            'A session MUST read [checkpoint](../checkpoint/GUIDE.md), and if a source is uncertain it MUST preserve evidence.',
        )
        for directive in directives:
            with self.subTest(directive=directive), World() as world:
                pack = self.copy(world)
                self.append(pack/'capture/GUIDE.md', '\n' + directive + '\n')
                value = self.check(world, pack)
                self.assertFalse(value['valid'], directive)
                self.assertEqual(value['issues'][0]['code'], 'loading-cycle')
        with World() as world:
            pack = self.copy(world)
            self.append(pack/'capture/GUIDE.md', '\nA session MUST read [bank](../bank/GUIDE.md).\n')
            self.assertTrue(self.check(world, pack)['valid'])

    def test_reference_forms_work_for_owner_bindings_and_root_routes_with_global_definitions(self):
        with World() as world:
            pack = self.copy(world)
            self.replace(pack/'checkpoint/GUIDE.md', '[capture domain](../capture/GUIDE.md)', '[capture domain][owner]')
            self.append(pack/'checkpoint/GUIDE.md', '\n[owner]: ../capture/GUIDE.md\n')
            self.replace(pack/'SKILL.md', '[checkpoint](checkpoint/GUIDE.md)', '[checkpoint][checkpoint-route]')
            self.append(pack/'SKILL.md', '\n[checkpoint-route]: checkpoint/GUIDE.md\n')
            self.assertTrue(self.check(world, pack)['valid'])

    def test_literal_marker_words_are_prose_and_cannot_index_internal_link_state(self):
        with World() as world:
            pack = self.copy(world)
            self.append(pack/'capture/GUIDE.md', '\nA session MUST read GUIDELINK999.\n')
            self.assertTrue(self.check(world, pack)['valid'])
            self.append(pack/'capture/GUIDE.md', '\nA session MUST read \x00L999\x00.\n')
            value = self.check(world, pack)
            self.assertFalse(value['valid'])
            self.assertEqual(value['issues'][0]['code'], 'markdown')

    def test_empty_literal_argument_is_data_not_a_hidden_shell_operator(self):
        with World() as world:
            pack = self.copy(world)
            self.append(pack/'SKILL.md', '\n```sh\ngit config example.note ""\n```\n')
            before = observed(pack)
            value = self.check(world, pack)
            self.assertTrue(value['valid'], value)
            self.assertEqual(observed(pack), before)

    def test_candidate_code_is_never_imported_or_executed_by_static_validation(self):
        with World() as world:
            pack = self.copy(world); marker = world.inside('must-not-exist')
            wrapper = pack/'scripts/checkpoint.py'
            self.append(wrapper, f'Path({str(marker)!r}).write_text("executed")\n')
            self.append(wrapper, '\nraise RuntimeError("untrusted candidate code")\n')
            value = self.check(world, pack)
            self.assertTrue(value['valid'], value)
            self.assertFalse(marker.exists())
            self.assertIn('behavioral correctness', ' '.join(value['limits']))

    def test_installed_delegation_checks_real_child_operations_and_flags_without_execution(self):
        cases = (
            'python3 -I "$SKILL_DIR/scripts/installer.py" checkpoint nonexistent',
            'python3 -I "$SKILL_DIR/scripts/installer.py" search --imaginary argument',
            'python3 -I "$SKILL_DIR/scripts/installer.py" boundary nonexistent',
        )
        for command in cases:
            with self.subTest(command=command), World() as world:
                pack = self.copy(world)
                self.append(pack/'maintain/cli/GUIDE.md', '\n```sh\n' + command + '\n```\n')
                before = observed(pack); value = self.check(world, pack)
                self.assertFalse(value['valid']); self.assertEqual(value['issues'][0]['code'], 'dispatch')
                self.assertEqual(observed(pack), before)

    def test_missing_delegated_helper_cannot_be_hidden_behind_the_installed_command(self):
        with World() as world:
            pack = self.copy(world)
            (pack/'scripts/retrieve.py').unlink()
            value = self.check(world, pack)
            self.assertFalse(value['valid']); self.assertEqual(value['issues'][0]['code'], 'dispatch')

    def test_working_delegated_help_commands_are_valid_documented_commands(self):
        # Actual independent #17 preflight false refusals on d32a0f4:
        # checkpoint/boundary help exit 0 but static delegation rejected them.
        for group in ('checkpoint', 'boundary', 'capture', 'identity'):
            with self.subTest(group=group), World() as world:
                pack = self.copy(world); before = observed(pack)
                result = world.run([sys.executable, '-I', pack/'scripts/installer.py', group, '--help'], check=False)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn('usage:', result.stdout); self.assertEqual(observed(pack), before)
                self.append(pack/'maintain/cli/GUIDE.md',
                            '\n```sh\npython3 -I "$SKILL_DIR/scripts/installer.py" ' + group + ' --help\n```\n')
                self.assertTrue(self.check(world, pack)['valid'], group)

    @staticmethod
    def append(path, text):
        path.write_text(path.read_text() + text)

    @staticmethod
    def replace(path, before, after):
        text = path.read_text()
        if before not in text:
            raise AssertionError('Fixture replacement does not match real source')
        path.write_text(text.replace(before, after))

    @staticmethod
    def replace_version(pack, version):
        path = pack/'SKILL.md'
        lines = [line for line in path.read_text().splitlines() if line.startswith('metadata: ')]
        if len(lines) != 1:
            raise AssertionError('Fixture requires one actual root metadata declaration')
        metadata = json.loads(lines[0].removeprefix('metadata: '))
        if not isinstance(metadata.get('version'), str) or not metadata['version']:
            raise AssertionError('Fixture requires the actual source version')
        metadata['version'] = version
        SkillRouteContract.replace(path, lines[0], 'metadata: ' + json.dumps(metadata, ensure_ascii=False))

    @staticmethod
    def script_without_execute(pack):
        (pack/'bin').mkdir(); path = pack/'bin/example'; path.write_text('#!/bin/sh\nexit 0\n'); path.chmod(0o644)


if __name__ == '__main__':
    unittest.main()
