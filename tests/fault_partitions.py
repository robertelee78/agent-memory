"""Complete, disjoint batches of independently replayed lifecycle interruptions."""
import json
import sys

PARTITIONS = 4
GROUPS = {}
for operation, mode in [('update', None), ('install', 'copy'), ('install', 'link')]:
    for phase in ('before', 'after'):
        name = f'update-write-{phase}' if operation == 'update' else f'install-write-{mode}-{phase}'
        GROUPS[name] = {'operation': operation, 'mode': mode, 'phase': phase,
                        'suites': [name if index == 0 else f'{name}-part-{index}'
                                   for index in range(PARTITIONS)]}
OWNERS = {suite: (name, index) for name, group in GROUPS.items()
          for index, suite in enumerate(group['suites'])}


def select(rows, partition):
    if partition is None:
        return rows
    if type(partition) is not int or not 0 <= partition < PARTITIONS:
        raise ValueError('Invalid lifecycle fault partition')
    if [row['step'] for row in rows] != list(range(len(rows))):
        raise ValueError('The complete observed fault inventory must be contiguous')
    selected = [row for row in rows if row['step'] % PARTITIONS == partition]
    if not selected:
        raise ValueError('A registered lifecycle fault partition cannot be empty')
    return selected


def emit(group, partition, rows, executed, artifact_digest):
    expected = [row['point'] for row in select(rows, partition)]
    if executed != expected:
        raise ValueError('Actual interruptions do not cover the selected observed writes')
    proof = {'group': group, 'index': partition, 'count': PARTITIONS,
             'artifact_digest': artifact_digest,
             'observed': [{key: row[key] for key in ('step', 'point', 'kind')} for row in rows],
             'executed': executed}
    print(json.dumps({'lifecycle_fault_partition': proof}), file=sys.stderr, flush=True)


def registered(entries):
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
    return matches[0]


def coverage(suites):
    by_name = {suite['name']: suite for suite in suites}
    summaries = []
    for name, group in GROUPS.items():
        observed = None; artifact = None; collected = []
        for index, suite_name in enumerate(group['suites']):
            suite = by_name.get(suite_name, {})
            proof = suite.get('fault_partition')
            if suite.get('status') != 'passed' or not isinstance(proof, dict):
                raise ValueError('Missing successful interruption partition: ' + suite_name)
            if (proof.get('group') != name or type(proof.get('index')) is not int or proof['index'] != index
                    or proof.get('count') != PARTITIONS or not isinstance(proof.get('observed'), list)):
                raise ValueError('Invalid lifecycle interruption partition binding: ' + suite_name)
            rows = proof['observed']
            if not rows or any(not isinstance(row, dict) or set(row) != {'step', 'point', 'kind'} for row in rows):
                raise ValueError('Incomplete observed interruption inventory: ' + suite_name)
            if (any(type(row['step']) is not int or not isinstance(row['point'], str)
                    or not isinstance(row['kind'], str) for row in rows)
                    or [row['step'] for row in rows] != list(range(len(rows)))):
                raise ValueError('Invalid observed interruption inventory: ' + suite_name)
            digest = proof.get('artifact_digest')
            if not isinstance(digest, str) or len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest):
                raise ValueError('Missing actual artifact identity for interruption coverage')
            if observed is None:
                observed = rows; artifact = digest
            elif rows != observed or digest != artifact:
                raise ValueError('Partitions observed different writes or runtime artifacts: ' + name)
            expected = [row['point'] for row in rows if row['step'] % PARTITIONS == index]
            if not expected or proof.get('executed') != expected:
                raise ValueError('Missing or extra actual interruption/recovery/retry: ' + suite_name)
            collected.extend(proof['executed'])
        points = [row['point'] for row in observed]
        if len(points) != len(set(points)) or sorted(collected) != sorted(points):
            raise ValueError('Lifecycle interruption coverage has duplicates or omissions: ' + name)
        summaries.append({'group': name, 'partitions': PARTITIONS, 'observed_points': len(points),
                          'actual_interruptions_and_retries': len(collected), 'artifact_digest': artifact})
    return summaries
