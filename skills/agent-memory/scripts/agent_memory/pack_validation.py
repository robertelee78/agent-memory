"""Read-only, non-executing checks of the portable instruction contract.

This checks the supported Markdown/argparse forms, not English equivalence,
program behavior, source trust, or the truth of memory. Candidate code is data.
"""
from __future__ import annotations

import argparse
import ast
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import stat
from urllib.parse import unquote, urlsplit

from .frontmatter import ContractError, read, unique_object

SECTIONS = ('Use when', 'Required context', 'Inputs and preconditions', 'Observe',
            'Plan', 'Apply', 'Read back', 'Verify', 'Outcomes', 'Failure handling',
            'Diagnostic fallback')
LINK = re.compile(r'!?\[([^\]\n]*)\]\((<[^>\n]+>|[^\s)]+)(?:\s+["\'][^\n]*["\'])?\)')
VERSION = re.compile(r'(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)'
                     r'(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?'
                     r'(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?')


class Invalid(ContractError):
    def __init__(self, code, path, detail):
        super().__init__(detail)
        self.issue = {'code': code, 'path': path, 'detail': detail}


def require(condition, code, path, detail):
    if not condition:
        raise Invalid(code, path, detail)


def semantic_version(value):
    match = VERSION.fullmatch(value) if isinstance(value, str) else None
    return bool(match) and all(not v.isdecimal() or len(v) == 1 or not v.startswith('0')
                               for v in (match[1] or '').split('.'))


def frontmatter(text, allowed, path):
    try:
        return read(text, allowed=allowed)
    except ContractError as exc:
        raise Invalid('frontmatter', path, str(exc)) from exc


def inventory(root):
    files = {}; size = 0; entries = 0
    for directory, dirs, names in os.walk(root, followlinks=False):
        entries += len(dirs) + len(names)
        require(entries <= 4096, 'inventory-bound', '.', 'Pack exceeds 4096 filesystem entries')
        for name in dirs + names:
            path = Path(directory) / name
            rel = path.relative_to(root).as_posix()
            mode = path.lstat().st_mode
            require(not stat.S_ISLNK(mode), 'file-type', rel, 'Internal symlinks are not portable assets')
            require(stat.S_ISREG(mode) or stat.S_ISDIR(mode), 'file-type', rel, 'Only regular files/directories are supported')
            if name in dirs:
                continue
            require(path.stat().st_size <= 2097152, 'inventory-bound', rel, 'Asset exceeds 2 MiB')
            with path.open('rb') as stream:
                raw = stream.read(2097153)
            size += len(raw)
            require(len(raw) <= 2097152 and size <= 16777216, 'inventory-bound', rel, 'Pack exceeds complete reader budget')
            require(mode & stat.S_IRUSR, 'asset-mode', rel, 'Assets must be owner-readable')
            if rel.startswith('bin/') or path.suffix == '.sh':
                require(mode & stat.S_IXUSR, 'asset-mode', rel, 'Directly executable assets need owner execute mode')
            files[rel] = (raw, stat.S_IMODE(mode))
    return files


def markdown(text):
    """Return prose and shell blocks; honor literal shorter nested fences."""
    prose = []; shell = []; block = []; fence = None; language = ''
    for line in text.splitlines():
        if fence:
            char, count = fence
            if re.fullmatch(r' {0,3}' + re.escape(char) + '{' + str(count) + r',}[ \t]*', line):
                if language in {'sh', 'bash', 'shell', 'zsh'}:
                    shell.append('\n'.join(block))
                fence = None; block = []
            else:
                block.append(line)
            prose.append('')
            continue
        match = re.match(r'^ {0,3}(`{3,}|~{3,})(.*)$', line)
        if match and not (match[1][0] == '`' and '`' in match[2]):
            fence = (match[1][0], len(match[1])); language = match[2].strip()
            prose.append('')
        else:
            prose.append(line)
    require(fence is None, 'markdown', '.', 'Unclosed code fence')
    return '\n'.join(prose), shell


def links(text):
    found = [(m[1], m[2].strip('<>')) for m in LINK.finditer(text)]
    definitions = {m[1].strip().casefold(): m[2].strip('<>') for m in
                   re.finditer(r'^ {0,3}\[([^\]]+)\]:\s*(<[^>]+>|\S+)', text, re.M)}
    for m in re.finditer(r'!?\[([^\]\n]+)\]\[([^\]\n]*)\]', text):
        key = (m[2] or m[1]).strip().casefold()
        require(key in definitions, 'link', '.', 'Undefined reference link')
        found.append((m[1], definitions[key]))
    for m in re.finditer(r'(?<!!)\[([^\]\n]+)\](?![\[(:])', text):
        key = m[1].strip().casefold()
        if key in definitions:
            found.append((m[1], definitions[key]))
    found.extend(('', m[1]) for m in re.finditer(r'<(?:a|img)\b[^>]*\b(?:href|src)=["\']([^"\']+)["\']', text, re.I))
    return found


def anchors(text):
    result = set(); counts = {}
    for match in re.finditer(r'^ {0,3}#{1,6} +(.+?)(?: +#+)?$', text, re.M):
        heading = re.sub(r'<[^>]*>', '', match[1]).lower()
        slug = re.sub(r'[^\w\- ]', '', heading).replace(' ', '-')
        number = counts.get(slug, 0); counts[slug] = number + 1
        result.add(slug if not number else slug + '-' + str(number))
    result.update(m[1] for m in re.finditer(r'<(?:a|span)\b[^>]*\bid=["\']([^"\']+)["\']', text, re.I))
    return result


def destination(source, target, files):
    parsed = urlsplit(target)
    if parsed.scheme or parsed.netloc:
        return None, ''
    path = unquote(parsed.path)
    require('\\' not in path and not path.startswith('/'), 'link', source, 'Relative links must stay inside the pack')
    parts = list(Path(source).parent.parts)
    for part in path.split('/') if path else []:
        if part in {'', '.'}:
            continue
        if part == '..':
            require(bool(parts), 'link', source, 'Link escapes the pack')
            parts.pop()
        else:
            parts.append(part)
    rel = '/'.join(parts) if path else source
    require(rel in files, 'link', source, 'Missing relative asset: ' + rel)
    return rel, unquote(parsed.fragment)


def dispatch(script, files):
    """Resolve a literal main import and static argparse commands, without import."""
    tree = ast.parse(files[script][0].decode('utf-8'), filename=script)
    imports = [n for n in ast.walk(tree) if isinstance(n, ast.ImportFrom)
               and n.module and n.module.startswith('agent_memory.')
               and any(a.name == 'main' for a in n.names)]
    require(len(imports) == 1, 'dispatch', script, 'Public Python helper needs one literal packaged main import')
    module = 'scripts/' + imports[0].module.replace('.', '/') + '.py'
    require(module in files, 'dispatch', script, 'Dispatch implementation is missing')
    tree = ast.parse(files[module][0].decode('utf-8'), filename=module)
    mains = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'main']
    require(len(mains) == 1, 'dispatch', script, 'Implementation needs one main function')
    commands = set(); flags = {'--help', '-h'}; subparsers = False
    def strings(node, names):
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            return [node.value]
        if isinstance(node, ast.Name):
            return names.get(node.id, [])
        if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Add):
            left, right = strings(node.left, names), strings(node.right, names)
            require(len(left) * len(right) <= 64, 'dispatch', script, 'Literal flag expansion exceeds bound')
            return [a + b for a in left for b in right]
        return []
    def visit(node, names):
        nonlocal subparsers
        if isinstance(node, ast.For) and isinstance(node.target, ast.Name):
            try:
                values = ast.literal_eval(node.iter)
            except (ValueError, TypeError):
                values = None
            if isinstance(values, (tuple, list)) and all(isinstance(v, str) for v in values):
                names = {**names, node.target.id: values}
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute):
            method = node.func.attr
            if method == 'add_subparsers':
                subparsers = True
            if method == 'add_parser' and node.args:
                arg = node.args[0]
                if isinstance(arg, ast.Constant) and isinstance(arg.value, str):
                    commands.add(arg.value)
                elif isinstance(arg, ast.Name) and arg.id in names:
                    commands.update(names[arg.id])
                else:
                    raise Invalid('dispatch', script, 'Unsupported dynamic argparse dispatch')
            if method == 'add_argument' and node.args:
                for arg in node.args:
                    flags.update(value for value in strings(arg, names) if value.startswith('-'))
                first = node.args[0]
                if isinstance(first, ast.Constant) and first.value in {'operation', 'command'}:
                    for kw in node.keywords:
                        if kw.arg == 'choices':
                            try:
                                commands.update(ast.literal_eval(kw.value))
                            except (ValueError, TypeError):
                                raise Invalid('dispatch', script, 'Operation choices must be literal')
        for child in ast.iter_child_nodes(node):
            visit(child, names)
    visit(mains[0], {})
    require(not subparsers or commands, 'dispatch', script, 'No supported static dispatch found')
    return module, commands, flags


def validate(pack):
    root = Path(pack).resolve(strict=True)
    require(root.is_dir(), 'inventory', '.', 'Pack must be a directory')
    files = inventory(root)
    require([p for p in files if Path(p).name == 'SKILL.md'] == ['SKILL.md'],
            'registration', '.', 'Exactly one root SKILL.md is required')
    documents = {}; metadata = {}; shells = {}; bodies = {}
    for path, (raw, mode) in sorted(files.items()):
        if path.startswith('schemas/') and path.endswith('.json'):
            json.loads(raw, object_pairs_hook=unique_object,
                       parse_constant=lambda _: (_ for _ in ()).throw(ValueError('Nonfinite JSON')))
        if not path.endswith('.md'):
            continue
        text = raw.decode('utf-8')
        if path == 'SKILL.md':
            meta, body = frontmatter(text, {'name', 'description', 'metadata', 'license', 'compatibility'}, path)
            require(meta.get('name') == 'agent-memory' and isinstance(meta.get('description'), str)
                    and 0 < len(meta['description']) <= 1024, 'frontmatter', path, 'Invalid discovery name/description')
            extra = meta.get('metadata')
            require(isinstance(extra, dict) and semantic_version(extra.get('version')),
                    'frontmatter', path, 'metadata.version must be a semantic version')
        elif Path(path).name == 'GUIDE.md':
            meta, body = frontmatter(text, {'kind', 'domain', 'file-operations'}, path)
            require(meta.get('kind') in {'domain', 'operation', 'model'}, 'frontmatter', path, 'Unknown guide kind')
            require(meta['kind'] == 'operation' or set(meta) == {'kind'},
                    'frontmatter', path, 'Only operations declare an owner or file-only operations')
        else:
            meta = {}; body = text
        metadata[path] = meta
        bodies[path] = body
        documents[path], shells[path] = markdown(body)
        shells[path].extend(m[1] for m in re.finditer(r'`(python3 [^`\n]+)`', documents[path]))
    graph = {p: set() for p in documents}; mandatory = {p: set() for p in documents}
    all_anchors = {p: anchors(t) for p, t in documents.items()}
    mappings = []; dispatches = {}; file_ops = {}; normative = 0
    for path, prose in documents.items():
        for match in re.finditer(r'^ {0,3}\[[^\]]+\]:\s*(<[^>]+>|\S+)', prose, re.M):
            destination(path, match[1].strip('<>'), files)
        for label, target in links(prose):
            dest, anchor = destination(path, target, files)
            if dest in graph:
                graph[path].add(dest)
            if anchor and dest:
                require(dest in all_anchors and anchor in all_anchors[dest], 'anchor', path, 'Missing anchor in ' + dest + ': ' + anchor)
    root_routes = documents['SKILL.md'].split('## Route\n', 1)
    require(len(root_routes) == 2, 'route', 'SKILL.md', 'Root needs its intent/domain/operation route table')
    for line in root_routes[1].split('\n## ', 1)[0].splitlines():
        if not line.startswith('|'):
            continue
        targets = [destination('SKILL.md', target, files)[0] for _, target in links(line)]
        if len(targets) == 2 and metadata.get(targets[1], {}).get('kind') == 'operation':
            owner = metadata[targets[1]].get('domain')
            if metadata.get(str(owner) + '/GUIDE.md', {}).get('kind') == 'domain':
                require(targets[0] == str(owner) + '/GUIDE.md', 'route', 'SKILL.md', 'Root route names the wrong primary domain')
    for path, prose in documents.items():
        for asset in re.findall(r'`((?:schemas|templates)/[^`\n]+)`', prose):
            require(asset in files, 'asset', path, 'Missing declared schema/template: ' + asset)
        meta = metadata[path]
        if meta.get('kind') == 'operation':
            headings = re.findall(r'^## (.+)$', prose, re.M)
            require(headings == list(SECTIONS), 'leaf-contract', path, 'Operation needs the eleven required sections in order')
            positions = [n for n, line in enumerate(prose.splitlines()) if line.startswith('## ')]
            original = bodies[path].splitlines()
            for index, heading in enumerate(SECTIONS):
                end = positions[index + 1] if index + 1 < len(positions) else len(original)
                part = '\n'.join(original[positions[index] + 1:end])
                require(bool(part.strip()), 'leaf-contract', path, 'Empty required section: ' + heading)
            domain = meta.get('domain')
            require(isinstance(domain, str) and re.fullmatch('[a-z][a-z0-9-]*', domain), 'domain', path, 'Operation needs one owning domain')
            parent = domain + '/GUIDE.md'
            require(metadata.get(parent, {}).get('kind') == 'domain', 'domain', path, 'Owning domain is missing')
            context = prose.split('## Required context\n', 1)[1].split('\n## ', 1)[0]
            resolved = {destination(path, target, files)[0] for _, target in links(context)}
            require(parent in resolved and 'MUST' in context, 'domain', path, 'Required context must bind the owning domain')
            require(path in graph[parent], 'domain', path, 'Owning domain must route to its operation')
            mandatory[path].add(parent)
        if 'file-operations' in meta:
            ops = meta['file-operations']
            require(meta.get('kind') == 'operation' and isinstance(ops, list) and ops
                    and all(isinstance(v, str) and re.fullmatch('[a-z][a-z0-9-]*', v) for v in ops)
                    and len(set(ops)) == len(ops), 'file-operation', path, 'File-only operations need explicit distinct names')
            file_ops[path] = ops
        # Recognize direct mandatory loading sentences. Intent/condition clauses
        # remain navigation, not mandatory edges. This is not an English parser.
        targets = []
        def substitute(match):
            targets.append(match[2].strip('<>'))
            return 'GUIDELINK' + str(len(targets) - 1)
        sentences = re.split(r'(?<=[.!?])\s+|\n\s*\n', LINK.sub(substitute, prose))
        for sentence in sentences:
            if re.search(r'\b(?:MUST|SHALL)\s+(?:always\s+)?(?:read|load|open|preload|follow)\s+(?:all|every|the (?:whole|entire))\b', sentence):
                raise Invalid('loading', path, 'Unconditional whole-tree loading is forbidden')
            if re.search(r'\b(?:when|if|unless|before|after|only|for|at|selected)\b', sentence, re.I):
                continue
            for match in re.finditer(r'\b(?:MUST|SHALL)\s+(?:first\s+)?(?:read|load|open|follow)(?: and follow)?\s+(?:the\s+)?GUIDELINK(\d+)', sentence):
                dest, _ = destination(path, targets[int(match[1])], files)
                if dest in mandatory:
                    mandatory[path].add(dest)
        clean = re.sub(r'`+[^`\n]*`+', '', prose)
        require(not re.search(r'\b(?:must|shall)\b', clean), 'rfc-form', path, 'Normative must/shall must use uppercase RFC forms')
        require(not re.search(r'\b(?:MUST|SHALL|SHOULD)\s+(?:not|may|should|must|shall)\b', clean), 'rfc-form', path, 'Ambiguous or mixed-case RFC form')
        require(not re.search(r'\b(?:MUST|SHALL|SHOULD)\s+(?:NOT\s+)?(?:MUST|SHALL|SHOULD|MAY|OPTIONAL|REQUIRED)\b', clean),
                'rfc-form', path, 'Compound RFC requirement form is ambiguous')
        normative += len(re.findall(r'\b(?:MUST|SHALL|SHOULD|MAY|REQUIRED|RECOMMENDED|OPTIONAL)\b', clean))
        if not path.startswith(('adapters/', 'lifecycle/')):
            require(not re.search(r'\b(?:Claude|Codex|OpenCode)\b', clean), 'client-neutrality', path, 'Core instructions must remain client-neutral')
        count = 0
        for block in shells[path]:
            for line in block.replace('\\\n', ' ').splitlines():
                line = line.strip()
                if not line or line.startswith('#'):
                    continue
                argv = shlex.split(line)
                if argv[0] == 'git':
                    require('config' in argv, 'dispatch', path, 'Unsupported external Git command form')
                    mappings.append({'guide': path, 'external': 'git', 'operation': 'config'})
                    continue
                require(argv[:2] == ['python3', '-I'] and len(argv) >= 3
                        and argv[2].startswith('$SKILL_DIR/scripts/'), 'dispatch', path, 'Undeclared shell command form')
                script = argv[2].removeprefix('$SKILL_DIR/')
                require(script in files, 'dispatch', path, 'Documented helper is missing: ' + script)
                if script not in dispatches:
                    dispatches[script] = dispatch(script, files)
                module, commands, flags = dispatches[script]
                verb = argv[3] if len(argv) > 3 and not argv[3].startswith('-') else None
                require((verb in commands) if commands else verb is None, 'dispatch', path, 'Unknown documented operation for ' + script)
                used_flags = {a.split('=')[0] for a in argv[3:] if a.startswith('--')}
                require(used_flags <= flags, 'dispatch', path, 'Unknown documented option for ' + script)
                mappings.append({'guide': path, 'script': script, 'module': module, 'operation': verb})
                count += 1
        if meta.get('kind') == 'operation':
            require(count or path in file_ops, 'dispatch', path, 'Operation needs a documented helper or declared file-only operation')
    reached = set(); pending = ['SKILL.md']
    while pending:
        path = pending.pop()
        if path not in reached:
            reached.add(path); pending.extend(graph[path])
    require(set(documents) == reached, 'reachability', '.', 'Unreachable guides: ' + ', '.join(sorted(set(documents) - reached)))
    visiting = set(); complete = set()
    def visit(path):
        require(path not in visiting, 'loading-cycle', path, 'Mandatory guide loading contains a cycle')
        if path in complete:
            return
        visiting.add(path)
        for child in mandatory[path]:
            visit(child)
        visiting.remove(path); complete.add(path)
    for path in mandatory:
        visit(path)
    require(files == inventory(root), 'changed-pack', '.', 'Pack changed during validation')
    return {'schema': 'agent-memory/pack-validation-v1', 'valid': True, 'writes_performed': False,
            'candidate_code_executed': False, 'semantic_completeness_verified': False,
            'version': metadata['SKILL.md']['metadata']['version'],
            'files': {p: {'sha256': hashlib.sha256(raw).hexdigest(), 'mode': oct(mode)} for p, (raw, mode) in sorted(files.items())},
            'guides': sorted(documents), 'mandatory_loads': {p: sorted(v) for p, v in mandatory.items()},
            'documented_commands': mappings, 'file_operations': file_ops, 'rfc_tokens_observed': normative,
            'limits': ['Supported Markdown links and literal argparse dispatch are mechanically checked.',
                       'Conditioned navigation is distinct from recognized direct mandatory loading sentences.',
                       'Parameter semantics, arbitrary English obligations and behavioral correctness require independent review.',
                       'Documentation refactors also require preserved source, rule/strength/location mapping and actual independent use.']}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pack', default=str(Path(__file__).resolve().parents[2]))
    args = parser.parse_args(argv)
    try:
        value = validate(args.pack)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, SyntaxError, RecursionError) as exc:
        value = {'schema': 'agent-memory/pack-validation-v1', 'valid': False,
                 'writes_performed': False, 'candidate_code_executed': False,
                 'semantic_completeness_verified': False,
                 'issues': [exc.issue if isinstance(exc, Invalid) else
                            {'code': 'malformed-pack', 'path': '.', 'detail': str(exc)}]}
    print(json.dumps(value, indent=2))
    return 0 if value['valid'] else 1
