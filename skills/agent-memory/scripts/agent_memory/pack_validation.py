"""Read-only, non-executing checks of the portable instruction contract.

This checks the supported Markdown/argparse forms, not English equivalence,
program behavior, source trust, or the truth of memory. Candidate code is data.
"""
from __future__ import annotations

import argparse
import ast
import hashlib
import html
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
VERSION = re.compile(r'(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)'
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
    def traversal_failed(exc):
        raise Invalid('inventory', '.', 'Every pack directory must be completely readable') from exc
    require(root.stat().st_mode & 0o500 == 0o500, 'asset-mode', '.', 'Pack root needs owner read/search mode')
    for directory, dirs, names in os.walk(root, followlinks=False, onerror=traversal_failed):
        entries += len(dirs) + len(names)
        require(entries <= 4096, 'inventory-bound', '.', 'Pack exceeds 4096 filesystem entries')
        for name in dirs + names:
            path = Path(directory) / name
            rel = path.relative_to(root).as_posix()
            mode = path.lstat().st_mode
            require(not stat.S_ISLNK(mode), 'file-type', rel, 'Internal symlinks are not portable assets')
            require(stat.S_ISREG(mode) or stat.S_ISDIR(mode), 'file-type', rel, 'Only regular files/directories are supported')
            if name in dirs:
                require(mode & 0o500 == 0o500, 'asset-mode', rel, 'Directories need owner read/search mode; no subtree may be skipped')
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


REFDEF = re.compile(r'^ {0,3}\[([^\]]+)\]:\s*(<[^>]+>|\S+)', re.M)


def reference_definitions(text):
    definitions = {}
    for match in REFDEF.finditer(text):
        key = ' '.join(match[1].split()).casefold(); target = match[2].strip('<>')
        require(key not in definitions or definitions[key] == target,
                'link', '.', 'Conflicting reference definitions')
        definitions[key] = target
    return definitions


def link_occurrences(text, references=None):
    """One occurrence resolver shared by navigation and loading checks."""
    require('\x00' not in text, 'markdown', '.', 'NUL is not supported in instruction prose')
    definitions = reference_definitions(text) if references is None else references
    # Preserve positions while masking literal inline code. Backticks inside a
    # link label do not hide its surrounding Markdown destination.
    masked = re.sub(r'(?<!`)(`+)(?!`)([^\n]*?)(?<!`)\1(?!`)',
                    lambda m: ' ' * len(m[0]), text)
    blocked = bytearray(len(masked)); found = []
    for match in REFDEF.finditer(masked):
        blocked[match.start():match.end()] = b'\1' * len(match[0])
    def add(match, label, target, image=False):
        start, end = match.span()
        if any(blocked[start:end]):
            return
        blocked[start:end] = b'\1' * (end - start)
        found.append((start, end, label, target, image))
    for match in re.finditer(r'<a\b[^>]*\bhref=["\']([^"\']+)["\'][^>]*>.*?</a\s*>', masked, re.I | re.S):
        add(match, '', html.unescape(match[1]))
    for match in re.finditer(r'<(?:a|img)\b[^>]*\b(?:href|src)=["\']([^"\']+)["\'][^>]*>', masked, re.I):
        add(match, '', html.unescape(match[1]), match[0].lower().startswith('<img'))
    for match in LINK.finditer(masked):
        add(match, match[1], match[2].strip('<>'), match[0].startswith('!'))
    for match in re.finditer(r'!?\[([^\]\n]+)\]\[([^\]\n]*)\]', masked):
        if any(blocked[match.start():match.end()]):
            continue
        key = ' '.join((match[2] or match[1]).split()).casefold()
        require(key in definitions, 'link', '.', 'Undefined reference link')
        add(match, match[1], definitions[key], match[0].startswith('!'))
    for match in re.finditer(r'!?\[([^\]\n]+)\](?![\[(:])', masked):
        key = ' '.join(match[1].split()).casefold()
        if key in definitions:
            add(match, match[1], definitions[key], match[0].startswith('!'))
    return sorted(found)


def links(text, references=None):
    return [(label, target) for _, _, label, target, image in link_occurrences(text, references) if not image]


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


RFC = re.compile(r'\b(?:MUST(?:\s+NOT)?|SHALL(?:\s+NOT)?|SHOULD(?:\s+NOT)?|'
                 r'NOT\s+RECOMMENDED|REQUIRED|RECOMMENDED|MAY|OPTIONAL)\b')
LOAD = r'\b(?:(?:first|always|also|unconditionally)\s+)*(?:read|load|open|preload|follow)'


def mandatory_targets(prose, path, files, references=None):
    """Supported positive load clauses; purpose words never imply a guard."""
    targets = []; result = set(); pieces = []; position = 0
    for start, end, _, target, image in link_occurrences(prose, references):
        pieces.append(prose[position:start])
        pieces.append(' ' if image else '\x00L' + str(len(targets)) + '\x00')
        targets.append(target); position = end
    pieces.append(prose[position:])
    text = ''.join(pieces)
    text = re.sub(r'`+[^`\n]*`+', '', text)
    text = REFDEF.sub('', text)
    # A newly introduced conditional clause cannot guard an earlier independent
    # directive. Keep genuine trailing and leading guards with their own clause.
    text = re.sub(r',?\s+\b(?:and|but|or|then)\s+(?=(?:if|when|unless)\b)', '\n\n', text, flags=re.I)
    sentences = re.split(r'(?<=[.!?;])\s+|\n\s*\n', text)
    for sentence in sentences:
        # Only explicit condition clauses and the protocol's named boundary
        # event suppress a mandatory edge. Bare for/only/at/selected do not.
        conditional = bool(
            re.search(r'\b(?:when|if|unless)\b', sentence, re.I)
            or re.search(r'\bat\s+(?:a\s+)?natural\s+(?:turn\s+)?boundar(?:y|ies)\b', sentence, re.I)
            or re.match(r'^\s*For\s+(?:qualified|eligible)\b', sentence, re.I))
        markers = list(RFC.finditer(sentence))
        for index, marker in enumerate(markers):
            if ' '.join(marker[0].split()) not in {'MUST', 'SHALL'}:
                continue
            end = markers[index + 1].start() if index + 1 < len(markers) else len(sentence)
            clause = sentence[marker.end():end]
            if conditional:
                continue
            require(not re.search(LOAD + r'\s+(?:all|every|the (?:whole|entire))\b', clause),
                    'loading', path, 'Unconditional whole-tree loading is forbidden')
            token = r'\x00L([0-9]+)\x00'
            for match in re.finditer(LOAD + r'(?:\s+and\s+follow)?\s+(?:the\s+)?' + token, clause):
                indexes = [int(match[1])]; tail = clause[match.end():]
                while (extra := re.match(r'\s*(?:,\s*(?:and\s+)?|and\s+)(?:the\s+)?' + token, tail)):
                    indexes.append(int(extra[1])); tail = tail[extra.end():]
                for index in indexes:
                    dest, _ = destination(path, targets[index], files)
                    if dest and dest.endswith('.md'):
                        result.add(dest)
    return result


def git_operation(argv, path):
    index = 1
    while index < len(argv) and argv[index] in {'-C', '-c', '--git-dir', '--work-tree'}:
        require(index + 1 < len(argv), 'dispatch', path, 'Missing Git global-option value')
        index += 2
    require(index < len(argv) and argv[index] == 'config', 'dispatch', path, 'Unsupported external Git operation')
    return argv[index]


def command_argv(line, path):
    require('$(' not in line and '`' not in line, 'dispatch', path, 'Command substitutions are outside the literal command contract')
    literal = re.sub(r'"\$SKILL_DIR/scripts/[a-z_]+\.py"', '', line)
    require('$' not in literal, 'dispatch', path, 'Only the quoted SKILL_DIR helper path may expand')
    unquoted = re.sub(r"'[^']*'|\"(?:\\.|[^\"\\])*\"", '', literal)
    require(not re.search(r'[*?\[\]{}~]', unquoted), 'dispatch', path, 'Unquoted shell expansions are outside the literal command contract')
    lexer = shlex.shlex(line, posix=True, punctuation_chars=';&|<>')
    lexer.whitespace_split = True; lexer.commenters = ''
    argv = list(lexer)
    require(bool(argv) and not any(t and t != '>' and set(t) <= set(';&|<>') for t in argv),
            'dispatch', path, 'Document one supported command per line, without hidden shell operations')
    if '>' in argv:
        require(argv.count('>') == 1 and len(argv) >= 3 and argv[-2] == '>',
                'dispatch', path, 'Only a trailing stdout-file redirection is supported')
        argv = argv[:-2]
    return argv


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
        for _, _, label, target, image in link_occurrences(prose):
            dest, anchor = destination(path, target, files)
            if dest in graph and not image:
                graph[path].add(dest)
            if anchor and dest:
                require(dest in all_anchors and anchor in all_anchors[dest], 'anchor', path, 'Missing anchor in ' + dest + ': ' + anchor)
    root_routes = documents['SKILL.md'].split('## Route\n', 1)
    require(len(root_routes) == 2, 'route', 'SKILL.md', 'Root needs its intent/domain/operation route table')
    for line in root_routes[1].split('\n## ', 1)[0].splitlines():
        if not line.startswith('|'):
            continue
        targets = [destination('SKILL.md', target, files)[0] for _, target in links(line, reference_definitions(documents['SKILL.md']))]
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
            require(parent in mandatory_targets(context, path, files, reference_definitions(prose)), 'domain', path,
                    'Required context must positively and unconditionally require its owning domain')
            require(path in graph[parent], 'domain', path, 'Owning domain must route to its operation')
            mandatory[path].add(parent)
        if 'file-operations' in meta:
            ops = meta['file-operations']
            require(meta.get('kind') == 'operation' and isinstance(ops, list) and ops
                    and all(isinstance(v, str) and re.fullmatch('[a-z][a-z0-9-]*', v) for v in ops)
                    and len(set(ops)) == len(ops), 'file-operation', path, 'File-only operations need explicit distinct names')
            file_ops[path] = ops
        mandatory[path].update(dest for dest in mandatory_targets(prose, path, files) if dest in mandatory)
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
                argv = command_argv(line, path)
                if argv[0] == 'git':
                    mappings.append({'guide': path, 'external': 'git', 'operation': git_operation(argv, path)})
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
        for child in sorted(mandatory[path]):
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
