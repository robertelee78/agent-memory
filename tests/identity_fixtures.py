"""Explicit public identity setup for tests that exercise mediated writers."""
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def bind(world, repo):
    def command(helper, *args):
        output = world.run([sys.executable, '-I', ROOT / 'scripts' / helper, *args])
        return json.loads(output.stdout)

    def apply(helper, proposal):
        path = world.inside('fixture-identity-proposal.json')
        path.write_text(json.dumps(proposal))
        return command(helper, 'apply', '--proposal', path)

    dev = command('developer_identity.py', 'status', '--repo', repo)
    if dev['setup_required']:
        apply('developer_identity.py', command('developer_identity.py', 'preview', '--create', '--label', 'Disposable test developer'))
    current = command('scope_binding.py', 'status', '--repo', repo)
    if not current['resolved']:
        proposal = command('scope_binding.py', 'preview', '--repo', repo, '--action', 'bind', '--reason', 'Explicit disposable fixture bank adoption')
        receipt = apply('scope_binding.py', proposal)
        if not receipt['verified']:
            raise AssertionError(receipt)
    return command('scope_binding.py', 'status', '--repo', repo)
