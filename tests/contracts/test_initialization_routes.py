"""Installed user outcomes for selective routes and preserved custom maps."""
import json
import unittest

from world import World
import installed_cli_fixtures as installed
from contracts import test_initialization as initialization
from contracts import test_checkpoint as checkpoint
from contracts import test_retrieval as retrieval


class InitializationRoutes(unittest.TestCase):
    def good(self, w, cli, *args):
        r, value = installed.command(w, cli, *args)
        self.assertEqual(r.returncode, 0, value or r.stderr)
        return value

    def helper(self, cls, cli):
        helper = cls()
        helper.command = lambda world, name, *args, **kw: installed.invoke(world, cli, name, *args, **kw)
        return helper

    def custom_router(self, bank):
        target = bank / 'MEMORY.md'
        text = target.read_text()
        original = next(line for line in text.splitlines() if '[projectbrief.md]' in line)
        custom = '|  [projectbrief.md](projectbrief.md)  | Project authority for this branch | branch:main | active | When reviewing branch requirements |'
        text = text.replace(original, custom).replace('# Project memory\n', '# Our reviewed project map\n\nRetain this custom introduction.\n')
        # Valid noncanonical metadata spelling must survive a missing-route add.
        text = text.replace('schema: "agent-memory/v1"', "schema: 'agent-memory/v1'")
        target.write_text(text)
        return text, custom

    def test_all_roles_are_selective_and_actual_map_supports_bounded_qualified_choices(self):
        with World() as w:
            cli = installed.install(w)
            h = self.helper(initialization.InitializationContract, cli)
            repo, project, _ = h.setup(w)
            (repo / 'README.md').write_text('# Widget\n\nCollaborating developers use a shared repository. Personal profiles are excluded.\n')
            task = 'https://github.com/fixture/widget/issues/1'
            awa = h.save(w, {'schema': 'agent-memory/awa-initialization-evidence-v1',
                'project_id': project, 'availability': 'available', 'reason': 'Reviewed isolated work fixture',
                'sources': [{'kind': 'project', 'ref': 'https://github.com/users/fixture/projects/1'},
                            {'kind': 'task', 'ref': task},
                            {'kind': 'milestone', 'ref': 'https://github.com/fixture/widget/milestone/1'}]}, 'awa.json')
            claims = [h.claim('intent', 5, 'docs/adr/one.md'),
                h.claim('requirement', 'shared repository, no personal profiles', 'README.md',
                        key='users-and-exclusions', context='productContext.md'),
                h.claim('implementation', 3, 'src/app.py', context='techContext.md'),
                h.claim('delivery', 'planned', task, context='progress.md',
                        summary='The retry task is planned; no delivered release is established.')]
            review = h.review(h.inventory(w, repo, awa), claims,
                [{'question': 'Which reviewed retry requirement governs the next implementation?',
                  'evidence': ['docs/adr/one.md']}])
            p = h.preview(w, repo, review)
            self.assertTrue(h.apply(w, p)[1]['verified'])
            checked = self.good(w, cli, 'bank', 'validate', '--repo', repo)
            routes = checked['routes']
            self.assertEqual(len(routes), 6)
            self.assertEqual(len({row['purpose'] for row in routes}), 6)
            self.assertEqual(len({row['read-when'] for row in routes}), 6)
            self.assertTrue(all('When starting or resolving project context' not in row['read-when'] for row in routes))
            active = next(row for row in routes if row['path'] == 'activeContext.md')
            self.assertIn('matching task', active['read-when'])
            self.assertNotIn('session start', active['read-when'])
            # Selection uses the actual produced map; no semantic query router is claimed.
            for need, path, evidence in [('product scope', 'productContext.md', 'README.md'),
                    ('implementation', 'techContext.md', 'src/app.py'),
                    ('delivery claims', 'progress.md', task)]:
                chosen = [row for row in routes if need in row['read-when']]
                self.assertEqual([row['path'] for row in chosen], [path])
                request = retrieval.RetrievalContract().request(project, query='', paths=[chosen[0]['path']],
                    kinds=[], include_context=True, limit=1)
                recalled = self.good(w, cli, 'get', '--repo', repo, '--request', h.save(w, request))
                self.assertEqual([item['path'] for item in recalled['results']], [path])
                item = recalled['results'][0]
                self.assertEqual(item['selection_class'], 'orientation-only')
                self.assertIsNone(item['trust'])
                self.assertFalse(item['source_assertions_verified'])
                self.assertIn(evidence, item['body'])
                self.assertLessEqual(len(item['body'].encode()), 16384)

    def test_adding_missing_context_preserves_custom_rows_prose_and_existing_files(self):
        with World() as w:
            cli = installed.install(w)
            h = self.helper(initialization.InitializationContract, cli)
            repo, _, _ = h.setup(w)
            self.assertTrue(h.apply(w, h.preview(w, repo, h.review(h.inventory(w, repo))))[1]['verified'])
            bank = repo / 'memory-bank'
            raw, custom = self.custom_router(bank)
            before = h.files(bank)
            claim = h.claim('requirement', 'shared', 'README.md', context='productContext.md')
            p = h.preview(w, repo, h.review(h.inventory(w, repo), [claim]))
            # Revalidation must reject substituted normalization of the old route.
            bad = json.loads(json.dumps(p))
            tx = bad['proposal']['transaction']
            tx['files']['MEMORY.md'] = tx['files']['MEMORY.md'].replace(custom, custom.replace('  ', ' '))
            self.assertNotEqual(h.apply(w, bad)[0].returncode, 0)
            self.assertEqual(h.files(bank), before)
            self.assertTrue(h.apply(w, p)[1]['verified'])
            after = h.files(bank)
            for path, body in before.items():
                if path != 'MEMORY.md':
                    self.assertEqual(after[path], body)
            router = after['MEMORY.md'].decode()
            self.assertIn(custom + '\n', router)
            self.assertIn("schema: 'agent-memory/v1'", router)
            self.assertEqual(raw.split('\n## Routes\n')[0].split('---\n', 2)[-1],
                             router.split('\n## Routes\n')[0].split('---\n', 2)[-1])
            for line in raw.splitlines():
                if line.startswith('|'):
                    self.assertIn(line + '\n', router)
                if line.startswith('- `') and not line.startswith('- `productContext.md`'):
                    self.assertIn(line + '\n', router)
            self.assertNotIn('- `productContext.md`:', router)
            self.assertEqual(set(p['changed_memory_paths']), {'memory-bank/MEMORY.md', 'memory-bank/productContext.md'})

    def test_checkpoint_preparation_uses_matching_task_profile_and_ordinary_save_keeps_router(self):
        with World() as w:
            cli = installed.install(w, 'link')
            h = self.helper(checkpoint.CheckpointContract, cli)
            repo, bank = h.fixture(w, False)
            _, custom = self.custom_router(bank)
            current = h.inspect(w, repo)
            p = h.good(w, 'checkpoint', 'prepare', '--repo', repo,
                       '--input', h.save(w, h.payload(current)))
            self.assertTrue(h.good(w, 'bank_transactions', 'apply', '--proposal', h.save(w, p['transaction']))['verified'])
            current = h.inspect(w, repo)
            self.assertIn('matching task', current['route']['read-when'])
            self.assertIn(custom + '\n', (bank / 'MEMORY.md').read_text())
            router = (bank / 'MEMORY.md').read_bytes()
            payload = h.payload(current)
            payload['next_step'] = 'Inspect a newly reported qualified result.'
            self.assertTrue(h.apply(w, h.preview(w, repo, payload))[1]['verified'])
            self.assertEqual((bank / 'MEMORY.md').read_bytes(), router)


if __name__ == '__main__':
    unittest.main()
