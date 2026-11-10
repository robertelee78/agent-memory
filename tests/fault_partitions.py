"""Complete, disjoint batches of independently replayed lifecycle interruptions."""
import json
import sys

PARTITIONS = 16
GROUPS = {}
for operation, mode in [('update', None), ('install', 'copy'), ('install', 'link'),
                        ('uninstall', 'copy'), ('uninstall', 'link')]:
    for phase in ('before', 'after'):
        name = f'update-write-{phase}' if operation == 'update' else f'{operation}-write-{mode}-{phase}'
        count = PARTITIONS
        GROUPS[name] = {'operation': operation, 'mode': mode, 'phase': phase, 'count': count,
                        'suites': [name if index == 0 else f'{name}-part-{index}'
                                   for index in range(count)]}
OWNERS = {suite: (name, index) for name, group in GROUPS.items()
          for index, suite in enumerate(group['suites'])}

_binding = None


def bind(suite, path, entries):
    """Bind one isolated worker's registered name, never an environment flag."""
    global _binding
    _binding = None
    if suite is None:
        return
    matches = [entry for entry in entries if entry['name'] == suite]
    if len(matches) != 1 or matches[0].get('path') != path:
        raise ValueError('Named suite does not identify this registered contract file')
    _binding = (suite, OWNERS.get(suite))


def selected_partition(group):
    """Unnamed direct runs retain the full inventory; named runs must own it."""
    if _binding is None:
        return None
    suite, owner = _binding
    if owner is None or owner[0] != group:
        raise ValueError('Registered suite does not own this lifecycle fault group: ' + suite)
    return owner[1]


def select(rows, partition, *, group=None):
    if partition is None:
        return rows
    if group is not None and group not in GROUPS:
        raise ValueError('Invalid lifecycle fault group')
    count = PARTITIONS
    if type(partition) is not int or not 0 <= partition < count:
        raise ValueError('Invalid lifecycle fault partition')
    if [row['step'] for row in rows] != list(range(len(rows))):
        raise ValueError('The complete observed fault inventory must be contiguous')
    selected = [row for row in rows if row['step'] % count == partition]
    if not selected:
        raise ValueError('A registered lifecycle fault partition cannot be empty')
    return selected


def emit(group, partition, rows, executed, artifact_digest):
    if group not in GROUPS or partition is None:
        raise ValueError('An actual partition proof requires a registered lifecycle fault group')
    expected = [row['point'] for row in select(rows, partition, group=group)]
    if executed != expected:
        raise ValueError('Actual interruptions do not cover the selected observed writes')
    proof = {'group': group, 'index': partition, 'count': GROUPS[group]['count'],
             'artifact_digest': artifact_digest,
             'observed': [{key: row[key] for key in ('step', 'point', 'kind')} for row in rows],
             'executed': executed}
    validate(proof, group, partition)
    print(json.dumps({'lifecycle_fault_partition': proof}), file=sys.stderr, flush=True)


def registered(entries):
    names = [entry['name'] for entry in entries]
    if len(names) != len(set(names)):
        raise ValueError('Lifecycle interruption registry contains duplicate suite names')
    by_name = {entry['name']: entry for entry in entries}
    required = set(OWNERS) | {'lifecycle-fault-partitions'}
    if not required <= set(by_name):
        raise ValueError('Every lifecycle interruption partition and its coverage control is required')
    if any(not by_name[name].get('path') or by_name[name].get('scope') != 'v0.0.1' for name in required):
        raise ValueError('Lifecycle interruption partitions cannot be pending or optional')


def extract(stderr, suite):
    group, index = OWNERS[suite]
    matches = []
    for line in stderr.splitlines():
        try:
            record = json.loads(line)
        except ValueError:
            continue
        if not isinstance(record, dict):
            continue
        proof = record.get('lifecycle_fault_partition')
        if isinstance(proof, dict) and proof.get('group') == group and proof.get('index') == index:
            matches.append(proof)
    if len(matches) != 1:
        raise ValueError('Expected one actual lifecycle interruption proof for ' + suite)
    validate(matches[0], group, index)
    return matches[0]


def validate(proof, group, index):
    if (proof.get('group') != group or type(proof.get('index')) is not int
            or proof['index'] != index or type(proof.get('count')) is not int
            or proof['count'] != PARTITIONS or not isinstance(proof.get('observed'), list)):
        raise ValueError('Invalid lifecycle interruption partition binding')
    rows = proof['observed']
    if not rows or any(not isinstance(row, dict) or set(row) != {'step', 'point', 'kind'} for row in rows):
        raise ValueError('Incomplete observed interruption inventory')
    if (any(type(row['step']) is not int or not isinstance(row['point'], str) or not row['point']
            or not isinstance(row['kind'], str) or not row['kind'] for row in rows)
            or [row['step'] for row in rows] != list(range(len(rows)))
            or len({row['point'] for row in rows}) != len(rows)):
        raise ValueError('Invalid observed interruption inventory')
    digest = proof.get('artifact_digest')
    if not isinstance(digest, str) or len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest):
        raise ValueError('Missing actual artifact identity for interruption coverage')
    expected = [row['point'] for row in select(rows, index, group=group)]
    if proof.get('executed') != expected:
        raise ValueError('Missing or extra actual interruption/recovery/retry')
    return rows, digest


def coverage(suites):
    names = [suite['name'] for suite in suites]
    if len(names) != len(set(names)):
        raise ValueError('Duplicate lifecycle interruption suite results')
    by_name = {suite['name']: suite for suite in suites}
    summaries = []
    for name, group in GROUPS.items():
        observed = None; artifact = None; collected = []; count = group['count']
        for index, suite_name in enumerate(group['suites']):
            suite = by_name.get(suite_name, {})
            proof = suite.get('fault_partition')
            if (suite.get('status') != 'passed' or type(suite.get('tests')) is not int
                    or suite['tests'] != 1 or not isinstance(proof, dict)):
                raise ValueError('Missing successful single-case interruption partition: ' + suite_name)
            rows, digest = validate(proof, name, index)
            if observed is None:
                observed = rows; artifact = digest
            elif rows != observed or digest != artifact:
                raise ValueError('Partitions observed different writes or runtime artifacts: ' + name)

            collected.extend(proof['executed'])
        points = [row['point'] for row in observed]
        if len(points) != len(set(points)) or sorted(collected) != sorted(points):
            raise ValueError('Lifecycle interruption coverage has duplicates or omissions: ' + name)
        summaries.append({'group': name, 'partitions': count, 'observed_points': len(points),
                          'actual_interruptions_and_retries': len(collected), 'artifact_digest': artifact})
    return summaries
