"""Installed-only command driver; unknown helper names never fall back to source."""
import json
from pathlib import Path

from contracts import test_installed_layout as layout


GROUPS = {
    'developer_identity': 'developer', 'project_identity': 'project',
    'scope_binding': 'identity', 'initialize': 'init', 'bank_contract': 'bank',
    'retrieve': 'search', 'assemble_context': 'context', 'admit_learning': 'admission',
    'compare_learning': 'compare', 'consolidate_learning': 'compact',
    'govern_memory': 'govern', 'checkpoint': 'checkpoint',
    'publish_memory': 'publish', 'refresh_memory': 'boundary',
    'reconcile_memory': 'semantic', 'sharing_boundary': 'sharing',
    'sharing_recovery': 'exchange', 'projection': 'projection',
    'bank_writes': 'writes', 'bank_transactions': 'transaction',
}


def install(world, mode='copy'):
    root, _ = layout.channel(world)
    result = layout.install(world, root, mode, 'all')
    if not result['verified']:
        raise AssertionError(result)
    return Path(result['receipt']['bin_dir']) / 'agent-memory'


def invoke(world, cli, helper, *args, env=None):
    name = Path(helper).stem
    if name not in GROUPS:
        raise AssertionError('Installed fixture has no declared route for ' + name)
    if name == 'admit_learning' and args and args[0] == 'capture':
        return command(world, cli, 'capture', *args[1:], env=env)
    return command(world, cli, GROUPS[name], *args, env=env)


def command(world, cli, *args, env=None):
    output = world.run([cli, *args], check=False, env=env, timeout=120)
    return output, json.loads(output.stdout) if output.stdout else None
