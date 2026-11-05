#!/usr/bin/env python3
"""Small disposable public recall/refresh smoke; not a performance qualification."""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / 'tests'))
from world import World
from refresh_fixtures import RefreshFixture
from contracts.test_retrieval import RetrievalContract


def main():
    with World() as world:
        fixture = RefreshFixture()
        repo, bank, peer, base = fixture.fixture(world)
        path = 'projectbrief.md'
        trace = fixture.good(world, 'discover_memory.py', 'start', '--repo', repo,
                             '--intent', 'recall', '--task', 'Read project orientation.')['discovery']
        trace.update(complete=True, rationale='Read the observed project orientation.',
                     sources=[{'path': path, 'sha256': hashlib.sha256((bank / path).read_bytes()).hexdigest()}])
        request = RetrievalContract().request(trace['binding']['project_id'], paths=[path], query='',
                                              include_context=True, discovery=trace)
        primed = fixture.good(world, 'retrieve.py', '--repo', repo, '--request', fixture.save(world, request))
        assert primed['metrics']['catalog_updated'] and not primed['optional_indexes_used'], primed
        pointer = next(world.paths['state'].rglob('recall-catalog-v1/current.json'))
        original = pointer.read_bytes()
        before = fixture.state(world, repo, bank)
        proposal = fixture.prepared_refresh(world, repo, recall=request)
        result = fixture.apply_refresh(world, proposal)
        assert result['refresh_applied'] and result['relevant_imports_read'], result
        assert result['recall']['optional_indexes_used'], result['recall']
        assert result['recall']['derived_cache_writes_performed'] is False
        later = fixture.good(world, 'retrieve.py', '--repo', repo, '--request', fixture.save(world, request))
        assert later['optional_indexes_used'] and not later['metrics']['catalog_updated']
        assert pointer.read_bytes() == original and fixture.state(world, repo, bank) == before
        print(json.dumps({'primed': True, 'refresh_hit': True, 'later_hit': True,
                          'same_catalog': True, 'preserved': True, 'selected': len(later['results'])}))


if __name__ == '__main__':
    main()
