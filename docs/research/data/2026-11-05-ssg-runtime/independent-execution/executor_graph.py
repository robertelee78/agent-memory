"""Exact-owned MAGE projection, parameter-only typed navigation and eligible ANN."""
from __future__ import annotations
import json
import time
import uuid
from executor_native import compact, digest
from executor_runtime import LABEL, command, free_port, memory_precondition, save

RELATIONS = {'corrects': 'CORRECTS', 'supersedes': 'SUPERSEDES', 'superseded-by': 'SUPERSEDED_BY',
             'derived-from': 'DERIVED_FROM', 'evidence': 'SUPPORTED_BY', 'scope': 'IN_SCOPE',
             'condition': 'HAS_CONDITION', 'origin': 'ATTRIBUTED_TO'}
EDGE_TYPES = tuple(RELATIONS.values()) + ('CONTAINS', 'ROUTES_TO')
NODE_TYPES = ('Project', 'RecordVersion', 'Context', 'Route', 'Scope', 'Condition', 'Evidence', 'Origin')


def projection(fixtures, bank):
    pid = fixtures.pid
    nodes, edges = {}, []
    project = 'project:' + pid
    nodes[project] = {'key': project, 'type': 'Project'}
    by_record = {(s.get('id'), s.get('version')): s for s in bank.sources.values() if 'id' in s}
    # Historical metadata may exist in the private graph, but never hidden/unsafe
    # record nodes or bodies. Fresh native endpoint sets fence every output path.
    visible = {}
    for path, s in bank.sources.items():
        if s['format'] == 'standalone-record' and (s['visibility'] != 'visible' or bank.scan.get(path, ['unscanned'])):
            continue
        visible[path] = s
        m = s.get('metadata', {})
        nodes[s['key']] = {'key': s['key'], 'type': 'RecordVersion' if 'id' in s else 'Context',
                           'path': path, 'source_sha256': s['file_sha256'], 'kind': m.get('kind', ''),
                           'component': m.get('conditions', {}).get('component') or '',
                           'status': m.get('status', ''), 'visibility': m.get('visibility', '')}

    def add(source, rel, target, target_type, properties=None):
        nodes.setdefault(target, {'key': target, 'type': target_type, **(properties or {})})
        edges.append({'from': source['key'], 'rel': rel, 'to': target, 'asserted_by': source['key'],
                      'source_sha256': source['file_sha256'], 'channel': 'recorded',
                      'generation': bank.generation_id})

    for source in visible.values():
        m = source.get('metadata')
        if not m:
            continue
        for field in ('corrects', 'supersedes', 'superseded-by', 'derived-from'):
            for ref in m.get(field, []):
                target = by_record.get((ref['id'], ref['version']))
                if target and target['path'] in visible:
                    add(source, RELATIONS[field], target['key'], 'RecordVersion')
        for ref in m.get('evidence', []):
            add(source, 'SUPPORTED_BY', 'evidence:' + pid + ':' + ref, 'Evidence')
        add(source, 'IN_SCOPE', 'scope:' + pid + ':' + m['scope'], 'Scope')
        for field, value in m.get('conditions', {}).items():
            add(source, 'HAS_CONDITION', 'condition:' + pid + ':' + compact([field, value]), 'Condition')
        origin = m.get('origin', {})
        if isinstance(origin, dict):
            for field, value in origin.items():
                if value is not None:
                    add(source, 'ATTRIBUTED_TO', 'origin:' + pid + ':' + compact([field, value]), 'Origin')
    routes = []
    for path, source in visible.items():
        text = bank.files[path]
        lines = [line for line in text.splitlines() if line.startswith('|')]
        if not lines or not lines[0].startswith('| Path | Purpose | Scope | Lifecycle | Read when |'):
            continue
        actual = fixtures.native.bank_contract.parse_table('\n'.join(lines), path)
        for ordinal, route in enumerate(actual):
            target = visible.get(route['path'])
            if target is None:
                continue
            key = 'route:' + pid + ':' + path + ':' + route['path'] + ':' + str(ordinal)
            value = {'key': key, 'type': 'Route', 'index_path': path, 'target_path': route['path'],
                     'ordinal': ordinal, 'source_sha256': source['file_sha256'], 'scope': route['scope']}
            nodes[key] = value
            routes.append(value)
            add(source, 'CONTAINS', key, 'Route')
            edges.append({'from': key, 'rel': 'ROUTES_TO', 'to': target['key'], 'asserted_by': source['key'],
                          'source_sha256': source['file_sha256'], 'channel': 'recorded', 'generation': bank.generation_id})
    for node in nodes.values():
        node.update(project=pid, generation=bank.generation_id)
    for edge in edges:
        edge['project'] = pid
    return {'nodes': list(nodes.values()), 'edges': edges, 'routes': routes,
            'generation_id': bank.generation_id, 'canonical_sha256': bank.canonical_sha256}


class Graph:
    def __init__(self, root, config, spec, run_id):
        self.root, self.config, self.spec, self.run_id = root, config, spec, run_id
        self.resources, self.driver = [], None
        self.active = None

    def call(self, *args, check=True):
        result = command([self.config['engine'], *args], check=check, timeout=120)
        path = self.root / ('engine-' + str(len(list(self.root.glob('engine-*.json'))) + 1) + '.json')
        save(path, result)
        return result

    def register(self, resource):
        self.resources.append(resource)
        save(self.root / 'resources.json', {'run_id': self.run_id, 'resources': self.resources})

    def inspect(self, resource):
        args = ['volume', 'inspect', resource['name']] if resource['type'] == 'volume' else ['inspect', resource.get('id', resource['name'])]
        result = self.call(*args, check=False)
        if result['exit']:
            listing = self.call(*(['volume', 'ls', '--format', '{{.Name}}'] if resource['type'] == 'volume'
                                  else ['ps', '-a', '--format', '{{.Names}}']))['stdout'].splitlines()
            if resource['name'] in listing:
                raise ValueError('Resource listing contradicts failed inspect; erasure pending')
            return None
        data = json.loads(result['stdout'])[0]
        labels = data.get('Labels') if resource['type'] == 'volume' else data.get('Config', {}).get('Labels')
        if not labels or labels.get(LABEL) != self.run_id:
            raise ValueError('Refusing foreign/recycled graph resource')
        if resource.get('id') and data['Id'] != resource['id']:
            raise ValueError('Container identity reused')
        if resource.get('created') and str(data.get('CreatedAt', data.get('Created'))) != resource['created']:
            raise ValueError('Volume identity reused')
        return data

    def start(self):
        from neo4j import GraphDatabase
        memory = memory_precondition(self.config)
        save(self.root / 'vm-memory-immediately-before-start.json', memory)
        if not memory['satisfied']:
            raise ValueError(memory['prerequisite'])
        name = 'am255-independent-' + self.run_id[:12] + '-' + uuid.uuid4().hex[:8]
        for role in ('data', 'logs'):
            resource = {'type': 'volume', 'name': name + '-' + role}
            if self.call('volume', 'inspect', resource['name'], check=False)['exit'] == 0:
                raise ValueError('New owned volume name exists; never adopt')
            self.register(resource)
            self.call('volume', 'create', '--label', LABEL + '=' + self.run_id, resource['name'])
            data = self.inspect(resource)
            resource['created'] = str(data.get('CreatedAt', data.get('Created')))
            save(self.root / 'resources.json', {'run_id': self.run_id, 'resources': self.resources})
        port = free_port()
        container = {'type': 'container', 'name': name}
        if self.call('inspect', name, check=False)['exit'] == 0:
            raise ValueError('New owned graph name exists; never adopt')
        self.register(container)
        self.call('run', '-d', '--pull=never', '--name', name, '--label', LABEL + '=' + self.run_id,
                  '--cpus=2', '--memory=12g', '-p', '127.0.0.1:' + str(port) + ':7687',
                  '-v', name + '-data:/var/lib/memgraph', '-v', name + '-logs:/var/log/memgraph',
                  self.spec['graph_profile']['image'], *self.spec['graph_profile']['flags'])
        data = self.inspect(container)
        container['id'] = data['Id']
        if data.get('Image', '').removeprefix('sha256:') != self.spec['graph_profile']['config_image']:
            raise ValueError('Running graph image configuration differs')
        save(self.root / 'resources.json', {'run_id': self.run_id, 'resources': self.resources})
        save(self.root / 'instance.json', data)
        user = {'uid': self.call('exec', container['id'], 'id', '-u')['stdout'].strip(),
                'gid': self.call('exec', container['id'], 'id', '-g')['stdout'].strip()}
        save(self.root / 'actual-image-user.json', user)
        if user != {'uid': '101', 'gid': '103'}:
            raise ValueError('Actual running MAGE UID/GID differs from pinned default')
        cgroup = {'memory_max': self.call('exec', container['id'], 'cat', '/sys/fs/cgroup/memory.max')['stdout'].strip(),
                  'cpu_max': self.call('exec', container['id'], 'cat', '/sys/fs/cgroup/cpu.max')['stdout'].strip()}
        save(self.root / 'actual-cgroup-ceilings.json', cgroup)
        cpu = cgroup['cpu_max'].split()
        if cgroup['memory_max'] != str(12 * 1024 ** 3) or len(cpu) != 2 or cpu[0] == 'max' or int(cpu[0]) != 2 * int(cpu[1]):
            raise ValueError('Actual graph cgroup ceilings differ from2CPUs/12GiB')
        self.driver = GraphDatabase.driver('bolt://127.0.0.1:' + str(port), auth=('', ''), connection_timeout=5,
                                          max_connection_pool_size=1)
        deadline = time.monotonic() + 60
        while True:
            try:
                self.query('RETURN 1 AS ok')
                break
            except Exception:
                if time.monotonic() > deadline or not self.inspect(container).get('State', {}).get('Running'):
                    self.call('logs', container['id'], check=False)
                    raise RuntimeError('Actual owned graph did not become ready; receipts retained')
                time.sleep(.2)
        save(self.root / 'procedures.json', self.query('CALL mg.procedures() YIELD * RETURN *'))
        self.query('CREATE INDEX ON :Source(key)')

    def freeze_plans(self, templates):
        plans = {}
        params = {'seed': '', 'qualified': [], 'component': '', 'component_prefix': '/',
                  'relations': [], 'offset': 0}
        for name, template in templates['templates'].items():
            plans[name] = {'cypher': template['cypher'], 'parameters': params,
                           'explain': self.query('EXPLAIN ' + template['cypher'], timeout=.25, **params)}
        save(self.root / 'frozen-explain-plans.json', plans)
        return plans

    def metrics(self):
        container = next((r for r in self.resources if r['type'] == 'container'), None)
        if container is None:
            return {'executed': False, 'prerequisite': 'No owned graph container started'}
        return self.call('stats', '--no-stream', '--format', 'json', container.get('id', container['name']), check=False)

    def query(self, text, timeout=None, **params):
        from neo4j import Query
        if self.driver is None:
            raise RuntimeError('No owned graph connection')
        with self.driver.session(database='memgraph') as session:
            return [r.data() for r in session.run(Query(text, timeout=timeout), **params)]

    def activate(self, projected, vectors, store):
        started = time.monotonic()
        # One physical namespace; erase old projection before loading its successor.
        removed = self.query('MATCH (n) DETACH DELETE n RETURN count(n) AS removed')
        if self.query('MATCH (n) RETURN count(n) AS count')[0]['count'] != 0:
            raise ValueError('Previous owned generation not completely removed')
        self.active = None
        for kind in NODE_TYPES:
            rows = [{k: v for k, v in n.items() if k != 'type'} for n in projected['nodes'] if n['type'] == kind]
            for start in range(0, len(rows), 256):
                self.query('UNWIND $rows AS row CREATE (n:Source:' + kind + ') SET n=row', rows=rows[start:start + 256])
        for rel in EDGE_TYPES:
            rows = [e for e in projected['edges'] if e['rel'] == rel]
            for start in range(0, len(rows), 256):
                self.query('UNWIND $rows AS row MATCH (a:Source {key:row.from}),(b:Source {key:row.to}) '
                           'CREATE (a)-[r:' + rel + ']->(b) SET r=row', rows=rows[start:start + 256])
        items = list(vectors.items())
        for start in range(0, len(items), 64):
            rows = [{'key': key, 'vector': store.array[index].tolist()} for key, index in items[start:start + 64]]
            self.query('UNWIND $rows AS row MATCH (n:Source {key:row.key}) SET n.embedding=row.vector', rows=rows)
        covered = self.query('MATCH (n:Source) RETURN count(n) AS count')[0]['count']
        if covered != len(projected['nodes']):
            raise ValueError('Actual graph projection cardinality mismatch')
        actual_keys = self.query('MATCH (n:Source) RETURN n.key AS key,n.project AS project,n.generation AS generation ORDER BY key')
        expected_keys = sorted([{'key': n['key'], 'project': n['project'], 'generation': n['generation']}
                                for n in projected['nodes']], key=lambda n: n['key'])
        if actual_keys != expected_keys:
            raise ValueError('Actual graph physical generation/source key coverage differs')
        embedded = self.query('MATCH (n:Source) WHERE n.embedding IS NOT NULL RETURN n.key AS key ORDER BY key')
        if [row['key'] for row in embedded] != sorted(vectors):
            raise ValueError('Actual graph vector coverage differs from full source/profile store')
        edge_count = self.query('MATCH ()-[r]->() RETURN count(r) AS count')[0]['count']
        if edge_count != len(projected['edges']):
            raise ValueError('Actual typed assertion coverage differs from canonical projection')
        self.active = projected['generation_id']
        receipt = {'generation_id': self.active, 'previous_removed': removed, 'node_count': covered,
                   'projected_node_count': len(projected['nodes']), 'edge_count': edge_count,
                   'embedded_count': len(embedded), 'actual_key_coverage_sha256': digest(compact(actual_keys).encode()),
                   'seconds': time.monotonic() - started}
        save(self.root / ('activation-' + str(len(list(self.root.glob('activation-*.json'))) + 1) + '.json'), receipt)
        return receipt

    def ann(self, qualified, vector, exact, ordinal):
        started = time.monotonic()
        keys = sorted(qualified)
        domain = digest(compact({'generation': self.active, 'keys': keys}).encode())
        index = 'am255_eligible_' + domain[:20]
        receipts = {'domain_sha256': domain, 'keys': keys, 'index': index}
        created = False
        try:
            self.query('MATCH (n:AM255Eligible) REMOVE n:AM255Eligible')
            self.query('UNWIND $keys AS key MATCH (n:Source {key:key}) SET n:AM255Eligible', keys=keys)
            actual = self.query('MATCH (n:AM255Eligible) RETURN n.key AS key ORDER BY key')
            if [r['key'] for r in actual] != keys:
                raise ValueError('Perfect-prefilter label differs from complete native domain')
            if not keys:
                return {'executed': True, 'domain_size': 0, 'candidate_budget': 16, 'reported_capacity': 0,
                        'raw_rows': [], 'exact_rescored': [], 'costs': {'seconds': time.monotonic() - started},
                        'interpretation': 'Empty freshly native-qualified domain; no ANN index/query required'}
            capacity = max(16, len(keys))
            build = time.monotonic()
            self.query('CREATE VECTOR INDEX ' + index + ' ON :AM255Eligible(embedding) WITH CONFIG ' +
                       compact({'dimension': 4096, 'capacity': capacity, 'metric': 'cos'}))
            created = True
            build_seconds = time.monotonic() - build
            info = self.query('CALL vector_search.show_index_info() YIELD * RETURN *')
            reported_capacity = next((row['capacity'] for row in info
                                      if index in row.values() and 'capacity' in row), None)
            raw = self.query('CALL vector_search.search($index, $budget, $vector) YIELD * RETURN *',
                             index=index, budget=min(16, len(keys)), vector=vector)
            rows = []
            for order, row in enumerate(raw):
                node = row['node']
                key = node['key']
                if key not in qualified:
                    raise ValueError('Actual ANN returned node outside complete eligible domain')
                rows.append({'key': key, 'engine_order': order, 'distance': row.get('distance'),
                             'similarity': row.get('similarity')})
            score_of = {r['key']: r['cosine'] for r in exact}
            rescored = sorted([{'key': r['key'], 'cosine': score_of[r['key']]} for r in rows],
                              key=lambda r: (-r['cosine'], r['key']))
            receipts['index_info_before_drop'] = info
            return {'executed': True, 'domain_size': len(keys), 'candidate_budget': 16,
                    'reported_capacity': reported_capacity, 'index_info': info,
                    'configured_capacity': capacity, 'raw_rows': rows,
                    'exact_rescored': rescored, 'costs': {'seconds': time.monotonic() - started,
                    'index_build_seconds': build_seconds}, 'domain_sha256': domain,
                    'interpretation': 'Perfect native-prefilter per-context ANN upper bound; not deployable eligible ANN. Raw SSG similarity is not accepted signed cosine.'}
        finally:
            if created:
                self.query('DROP VECTOR INDEX ' + index)
            self.query('MATCH (n:AM255Eligible) REMOVE n:AM255Eligible')
            receipts['indexes_after_drop'] = self.query('CALL vector_search.show_index_info() YIELD * RETURN *')
            receipts['label_count_after_removal'] = self.query('MATCH (n:AM255Eligible) RETURN count(n) AS count')[0]['count']
            if receipts['label_count_after_removal'] != 0 or any(index in str(row) for row in receipts['indexes_after_drop']):
                raise ValueError('Owned domain index/label disposal incomplete')
            save(self.root / ('ann-disposal-' + str(ordinal) + '.json'), receipts)

    def typed(self, fixtures, bank, case, native, templates):
        params = case['template']
        missing = lambda reason: {'executed': False, 'prerequisite': reason, 'edges': [], 'primary_candidates': [], 'route_pointers': [], 'costs': {}}
        if params.get('historical'):
            return missing('Native AM175 has no reviewed historical endpoint qualification mapper; historical typed cell unexecuted')
        seed = fixtures.resolve(params['seed'], bank.files, list(bank.by_label.values()))
        seed_key = fixtures.record_key(seed['metadata'])
        qualified = {r['key'] for r in native['eligible']}
        if seed_key not in qualified:
            return missing('Seed is not freshly native-qualified current guidance; no hidden/unsafe/historical endpoint disclosure')
        name = case['operation'].split(':', 1)[1]
        relations = params.get('relations', 'all-recorded')
        if relations == ['co-routed']:
            name = 'co-routed'
        elif relations == ['origin']:
            name = 'origin'
        allowed = [value for field, value in RELATIONS.items() if field != 'origin'] if relations == 'all-recorded' else [
            RELATIONS[r] for r in relations if r in RELATIONS and r != 'origin']
        value = templates['templates'][name]
        args = {'seed': seed_key, 'qualified': sorted(qualified), 'component': params.get('component', ''),
                'component_prefix': params.get('component', '') + '/', 'relations': allowed, 'offset': 0}
        start = time.monotonic()
        with self.driver.session(database='memgraph') as session:
            from neo4j import Query
            explained = session.run(Query('EXPLAIN ' + value['cypher'], timeout=.25), **args)
            plan = [r.data() for r in explained]
            rows = [dict(r.items()) for r in session.run(Query(value['cypher'], timeout=.25), **args)]
        edges, candidates, pointers = {}, set(), []
        for row in rows:
            if name == 'co-routed':
                route = next(n for n in self.projected['routes'] if n['key'] == row['key'])
                entry = bank.entry_of[route['index_path']]
                safe = not fixtures.native.security.scan(bank.files[route['index_path']] + '\n' +
                    json.dumps({'path': route['index_path'], 'scope_chain': entry['scope_chain']}, ensure_ascii=False))['reasons']
                if safe and entry['lifecycle'] == 'active' and not fixtures.native.retrieval.scope_reasons(fixtures.pid, entry, self.context):
                    pointers.append(row['key'])
                continue
            path = row['p']
            for rel in path.relationships:
                edge = {'from': rel.start_node['key'], 'rel': rel.type, 'to': rel.end_node['key'],
                        'asserted_by': rel['asserted_by'], 'source_sha256': rel['source_sha256'], 'channel': 'recorded'}
                edges[compact(edge)] = edge
            if name != 'origin':
                candidates.update(n['key'] for n in path.nodes if 'RecordVersion' in n.labels and n['key'] != seed_key)
        return {'executed': True, 'edges': list(edges.values()), 'primary_candidates': sorted(candidates),
                'route_pointers': sorted(pointers), 'costs': {'seconds': time.monotonic() - start,
                'database_timeout_seconds': .25, 'clean_performance_acceptance': False},
                'explain': plan, 'template': name, 'parameters_sha256': digest(compact(args).encode()),
                'interpretation': 'Origin/co-routing are provenance/navigation only' if name in ('origin', 'co-routed') else 'Source-asserted recorded navigation; no outcome/caveat mapping'}

    def cleanup(self):
        if self.driver:
            self.driver.close()
            self.driver = None
        rows = []
        for resource in sorted(self.resources, key=lambda r: r['type'] != 'container'):
            try:
                if self.inspect(resource):
                    if resource['type'] == 'container':
                        self.call('logs', resource.get('id', resource['name']), check=False)
                    self.call(*(['rm', '-f', resource.get('id', resource['name'])] if resource['type'] == 'container'
                                else ['volume', 'rm', resource['name']]))
                rows.append({'resource': resource, 'absent': self.inspect(resource) is None})
            except Exception as exc:
                rows.append({'resource': resource, 'absent': False, 'error': str(exc)})
        result = {'resources': rows, 'status': 'disposed' if all(r['absent'] for r in rows) else 'erasure-pending',
                  'borrowed_artifacts_untouched': True, 'forensic_erasure_claim': False}
        save(self.root / 'cleanup.json', result)
        return result
