"""Inspect bundled migration declarations and code names without executing them."""
import ast
import hashlib
import json
import re

from .frontmatter import ContractError, unique_object


def require(value, message):
    if not value:
        raise ContractError(message)


def entry(pack, value, files):
    require(isinstance(value, dict) and set(value) == {'module', 'function'}, 'Invalid migration code declaration')
    name, function = value['module'], value['function']
    require(isinstance(name, str) and re.fullmatch(r'scripts/agent_memory/migration_[a-z0-9_]+\.py', name)
            and name in files and isinstance(function, str) and re.fullmatch(r'[a-z][a-z0-9_]{0,63}', function),
            'Migration code must name a bundled module and explicit function')
    raw = (pack / name).read_bytes()
    require(hashlib.sha256(raw).hexdigest() == files[name]['sha256'], 'Migration code changed during declaration inspection')
    definitions = [node for node in ast.parse(raw).body if isinstance(node, ast.FunctionDef) and node.name == function]
    require(len(definitions) == 1 and not definitions[0].decorator_list
            and len(definitions[0].args.posonlyargs + definitions[0].args.args) == 1
            and not definitions[0].args.vararg and not definitions[0].args.kwarg
            and not definitions[0].args.kwonlyargs,
            'Migration entry must be one undecorated bundled function accepting the complete file map')
    return {**value, 'sha256': files[name]['sha256']}


def route(catalog, source, target):
    memo = {}
    def visit(schema, active):
        require(schema not in active, 'Migration catalog contains a cycle')
        if schema == target:
            return []
        if schema in memo:
            return memo[schema]
        found = None
        for step in catalog['steps']:
            if step['from'] == schema:
                suffix = visit(step['to'], {*active, schema})
                if suffix is not None:
                    require(found is None, 'Migration catalog contains an ambiguous conversion path')
                    found = [step, *suffix]
        memo[schema] = found
        return found
    return visit(source, set())


def inspect(pack, compatibility, files):
    name = compatibility.get('migration_catalog')
    if name is None:
        require(not compatibility['migrator_source_schemas'], 'Advertised migrators require a complete bundled catalog')
        return None
    require(name == 'schemas/migrations.json' and name in files, 'Migration catalog must be a complete bundled declaration')
    raw = (pack / name).read_bytes()
    require(len(raw) <= 65536 and hashlib.sha256(raw).hexdigest() == files[name]['sha256'], 'Migration catalog is oversized or changed')
    value = json.loads(raw, object_pairs_hook=unique_object,
        parse_constant=lambda _: (_ for _ in ()).throw(ContractError('Nonfinite migration catalog')))
    require(isinstance(value, dict) and set(value) == {'schema', 'codecs', 'steps'}
            and value['schema'] == 'agent-memory/migration-catalog-v1'
            and isinstance(value['codecs'], list) and 1 <= len(value['codecs']) <= 64
            and isinstance(value['steps'], list) and len(value['steps']) <= 64, 'Unsupported migration catalog shape')
    codecs = {}
    for codec in value['codecs']:
        require(isinstance(codec, dict) and set(codec) == {'schema', 'validator'}
                and isinstance(codec['schema'], str) and re.fullmatch(r'agent-memory/[A-Za-z0-9_.-]{1,128}', codec['schema'])
                and codec['schema'] not in codecs, 'Invalid or repeated migration codec')
        codecs[codec['schema']] = entry(pack, codec['validator'], files)
    steps, ids = [], set()
    for step in value['steps']:
        require(isinstance(step, dict) and set(step) == {'id', 'from', 'to', 'converter'}
                and isinstance(step['id'], str) and re.fullmatch(r'[a-z0-9][a-z0-9_.-]{0,127}', step['id'])
                and step['id'] not in ids and step['from'] in codecs and step['to'] in codecs
                and step['from'] != step['to'], 'Invalid, incomplete or repeated migration step')
        ids.add(step['id']); steps.append({**step, 'converter': entry(pack, step['converter'], files)})
    catalog = {'schema': value['schema'], 'sha256': files[name]['sha256'], 'codecs': codecs, 'steps': steps}
    readable = set(compatibility['readable_bank_schemas'])
    require(readable <= set(codecs), 'Readable schemas require complete bundled validators')
    # Detect cycles even in a disconnected part of the graph.
    complete = set()
    def acyclic(schema, active):
        require(schema not in active, 'Migration catalog contains a cycle')
        if schema in complete:
            return
        for step in steps:
            if step['from'] == schema:
                acyclic(step['to'], {*active, schema})
        complete.add(schema)
    for schema in codecs:
        acyclic(schema, set())
    supported = {schema for schema in codecs if schema not in readable
                 and any(route(catalog, schema, target) is not None for target in readable)}
    require(supported == set(compatibility['migrator_source_schemas']),
            'Advertised migration source schemas differ from complete bundled chains')
    # Check every requested pair, including currently readable sources.
    for source in codecs:
        for target in readable:
            route(catalog, source, target)
    return catalog
