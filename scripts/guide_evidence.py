#!/usr/bin/env python3
"""Capture normative guide rules/routes and verify an explicit refactor mapping.

This is repository maintenance tooling, not a semantic equivalence detector.
Changed wording requires an actual independent review of the supplied mapping.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

SCHEMA='agent-memory/guide-evidence-v1'
TOKENS=re.compile(r'\b(?:MUST\s+NOT|SHALL\s+NOT|SHOULD\s+NOT|NOT\s+RECOMMENDED|MUST|SHALL|REQUIRED|SHOULD|RECOMMENDED|MAY|OPTIONAL)\b')
FORCE={'MUST':('positive',3),'SHALL':('positive',3),'REQUIRED':('positive',3),
       'SHOULD':('positive',2),'RECOMMENDED':('positive',2),'MAY':('positive',1),'OPTIONAL':('positive',1),
       'MUST NOT':('negative',3),'SHALL NOT':('negative',3),'SHOULD NOT':('negative',2),'NOT RECOMMENDED':('negative',2)}


def sha(raw):return hashlib.sha256(raw).hexdigest()


def forces(text):
    return [list(FORCE[' '.join(token.split())]) for token in TOKENS.findall(text)]


def outside_code(text):
    lines=[];fence=None
    for line in text.splitlines():
        if fence is not None:
            char,length=fence
            if re.fullmatch(r' {0,3}'+re.escape(char)+'{'+str(length)+r',}[ \t]*',line):fence=None
            lines.append('');continue
        match=re.match(r'^ {0,3}(`{3,}|~{3,})(.*)$',line)
        if match and not (match[1][0]=='`' and '`' in match[2]):
            fence=(match[1][0],len(match[1]));lines.append('');continue
        lines.append(line)
    if fence:raise ValueError('Unclosed code fence')
    return '\n'.join(lines)


def snapshot(pack):
    pack=Path(pack).resolve()
    if not (pack/'SKILL.md').is_file():raise ValueError('A progressive pack requires SKILL.md')
    files={};rules={};links={}
    for path in sorted(pack.rglob('*.md')):
        if path.is_symlink() or not path.resolve().is_relative_to(pack):raise ValueError('Guide paths must stay inside the pack')
        raw=path.read_bytes()
        if len(raw)>262144:raise ValueError('Guide exceeds bounded evidence reader')
        relative=path.relative_to(pack).as_posix();text=outside_code(raw.decode('utf-8'))
        files[relative]=sha(raw);links[relative]=[]
        for match in re.finditer(r'(?:^|\n\s*\n)([^\n].*?)(?=\n\s*\n|\Z)',text,re.S):
            paragraph=match[1].strip();tokens=TOKENS.findall(paragraph)
            if not tokens:continue
            line=text[:match.start(1)].count('\n')+1
            key=sha((relative+'\0'+str(line)+'\0'+paragraph).encode())
            rules[key]={'path':relative,'line':line,'text':paragraph,'force':forces(paragraph)}
        for match in re.finditer(r'(?<!!)\[[^\]]*\]\(([^\n)]+)\)',text):
            target=match[1].strip()
            if target.startswith('<') and target.endswith('>'):target=target[1:-1]
            parsed=urlsplit(target)
            if parsed.scheme or parsed.netloc or not parsed.path:continue
            destination=(path.parent/unquote(parsed.path)).resolve()
            if not destination.is_relative_to(pack):raise ValueError('A runtime guide link escapes the portable pack')
            if not destination.is_file():raise ValueError('A runtime guide link is missing')
            if destination.suffix=='.md':links[relative].append(destination.relative_to(pack).as_posix())
    reached=set();pending=['SKILL.md']
    while pending:
        path=pending.pop()
        if path in reached:continue
        if path not in links:raise ValueError('Route points outside the guide inventory')
        reached.add(path);pending.extend(links[path])
    if set(files)-reached:raise ValueError('Every runtime Markdown guide must remain reachable from SKILL.md')
    return {'schema':SCHEMA,'files':files,'rules':rules,'routes':links,'reachable':sorted(reached),
            'source_origin_authenticated':False,'semantic_equivalence_verified':False}


def validate_before(before):
    if not isinstance(before,dict) or before.get('schema')!=SCHEMA or not isinstance(before.get('rules'),dict):
        raise ValueError('Use the complete prior observed guide snapshot')
    for key,rule in before['rules'].items():
        if (set(rule)!={'path','line','text','force'} or not isinstance(rule['text'],str)
                or key!=sha((rule['path']+'\0'+str(rule['line'])+'\0'+rule['text']).encode())
                or rule['force']!=forces(rule['text'])):
            raise ValueError('Prior rule evidence was substituted or is incomplete')


def sufficient(old,new):
    required=Counter(tuple(force) for force in old);available=Counter(tuple(force) for force in new)
    for (polarity,strength),count in sorted(required.items(),key=lambda item:item[0][1],reverse=True):
        for candidate in range(strength,4):
            used=min(count,available[polarity,candidate]);count-=used;available[polarity,candidate]-=used
        if count:return False
    return True


def check(before,after,mapping):
    validate_before(before)
    if not isinstance(mapping,dict) or set(mapping)-set(before['rules']):raise ValueError('Rule map names unknown prior rules')
    identical={}
    for key,rule in after['rules'].items():identical.setdefault(rule['text'],[]).append(key)
    coverage=[]
    for key,old in before['rules'].items():
        if key in mapping:
            entry=mapping[key]
            if (not isinstance(entry,dict) or set(entry)!={'new_rules','reason','meaning_preserved'}
                    or not isinstance(entry['new_rules'],list) or not entry['new_rules']
                    or len(set(entry['new_rules']))!=len(entry['new_rules'])
                    or any(target not in after['rules'] for target in entry['new_rules'])
                    or not isinstance(entry['reason'],str) or not entry['reason'].strip()
                    or entry['meaning_preserved'] is not True):raise ValueError('Changed rules require explicit actual meaning review and exact target rules')
            selected=entry['new_rules'];kind='reviewed-wording-map'
        elif old['text'] in identical:
            selected=[identical[old['text']][0]];kind='identical-text'
        else:raise ValueError('A prior normative instruction is missing its explicit preservation map')
        if not sufficient(old['force'],[force for target in selected for force in after['rules'][target]['force']]):
            raise ValueError('A normative instruction lost force or changed obligation/prohibition polarity')
        coverage.append({'old_rule':key,'new_rules':selected,'method':kind})
    return {'schema':'agent-memory/guide-refactor-result-v1','passed':True,'rule_map':coverage,
            'old_rules':len(before['rules']),'new_rules':len(after['rules']),'reachable_guides':after['reachable'],
            'semantic_equivalence_verified':False,'changed_meaning_is_reviewer_assertion':True,
            'prior_source_origin_authenticated':False,'writes_performed':False}


def read_json(path):
    raw=Path(path).read_bytes()
    if len(raw)>4194304:raise ValueError('Evidence input exceeds complete reader budget')
    return json.loads(raw)


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='command',required=True)
    a=sub.add_parser('snapshot');a.add_argument('--pack',required=True)
    a=sub.add_parser('check');a.add_argument('--pack',required=True);a.add_argument('--before',required=True);a.add_argument('--before-pack',required=True);a.add_argument('--map')
    args=parser.parse_args(argv)
    try:
        after=snapshot(args.pack)
        if args.command=='snapshot':value=after
        else:
            before=read_json(args.before)
            if before!=snapshot(args.before_pack):raise ValueError('Prior snapshot does not match the preserved actual source')
            value=check(before,after,read_json(args.map) if args.map else {})
        print(json.dumps(value,indent=2));return 0
    except (OSError,ValueError,KeyError,TypeError,UnicodeError):
        print(json.dumps({'schema':'agent-memory/guide-refactor-result-v1','passed':False,'writes_performed':False,
                          'error':'Missing, changed, unreachable or weakened guide evidence; preserve source and review the complete map'}));return 1


if __name__=='__main__':sys.exit(main())
