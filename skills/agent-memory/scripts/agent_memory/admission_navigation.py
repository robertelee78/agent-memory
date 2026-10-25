"""Apply agent-reviewed useful captions/placement without replacing source payloads."""
from . import bank_contract as bank, bank_layout as layout, scopes, security
from .frontmatter import ContractError, read, write
from .developer_identity import encoded


def caption(value):
    if not isinstance(value,str) or not value.strip() or len(value.encode())>512:
        raise ContractError('Navigation needs a bounded meaningful subject and read trigger')
    if security.scan(value)['reasons']:raise ContractError('Unsafe route caption needs clarification')
    return value


def placement(value,*,index=False):
    fields={'parent','purpose','read-when'}|({'path','scope'} if index else set())
    if not isinstance(value,dict) or set(value)!=fields:
        raise ContractError('Use explicit parent, purpose/read trigger and native index declarations')
    if value['parent']!='MEMORY.md':bank.path_value(value['parent'])
    for field in ('purpose','read-when'):caption(value[field])
    if index:
        bank.path_value(value['path'])
        if not value['path'].startswith('topics/') or value['path'] in bank.CONTEXTS:
            raise ContractError('New topic nodes stay in the native topic-index namespace')
        scopes.parse_scope(value['scope'])
        if value['scope']!='repository' and not scopes.parse_scope(value['scope']):
            raise ContractError('Declare an actual inherited topic scope')
    return value


def navigation_value(value):
    if (not isinstance(value,dict) or set(value)!={'learning','history','indexes'}
            or not isinstance(value['indexes'],list) or len(value['indexes'])>8 or len(encoded(value))>16384):
        raise ContractError('Use a small explicit reviewed learning/history navigation plan')
    placement(value['learning']);placement(value['history'])
    paths=[]
    for node in value['indexes']:placement(node,index=True);paths.append(node['path'])
    if len(paths)!=len(set(paths)):raise ContractError('New topic nodes require distinct canonical paths')
    return value


def route(path,plan,scope,state='active'):
    return {'path':path,'purpose':plan['purpose'],'scope':scope,'lifecycle':state,'read-when':plan['read-when']}


def apply(files,navigation,learning,history,metadata,stamp):
    navigation_value(navigation)
    current=dict(files);root_meta,root_routes,omitted=bank.parse_router(files['MEMORY.md'])
    headers={'MEMORY.md':root_meta};tables={'MEMORY.md':list(root_routes)};bodies={}
    for entry in bank.validate_files(files)['entries']:
        if entry['format']=='topic-index':
            meta,body=read(files[entry['path']],allowed=layout.SPEC['properties'])
            headers[entry['path']]=meta;tables[entry['path']]=layout.parse_index(entry['path'],meta,body)
    new=set()
    for node in navigation['indexes']:
        path=node['path']
        if path in current or path in tables:raise ContractError('New index declaration must not replace an existing source')
        headers[path]={'schema':layout.SPEC['$id'],'project-id':metadata['project-id'],'updated':stamp}
        tables[path]=[];new.add(path)
    def add(parent,item):
        if parent not in tables:raise ContractError('The chosen parent must be an actual existing or declared topic node')
        if any(row['path']==item['path'] for row in tables[parent]):raise ContractError('A new route must not replace an existing route')
        tables[parent].append(item)
    for node in navigation['indexes']:add(node['parent'],route(node['path'],node,node['scope']))
    add(navigation['learning']['parent'],route(learning,navigation['learning'],metadata['scope']))
    add(navigation['history']['parent'],route(history,navigation['history'],'repository'))
    touched={p['parent'] for p in [*navigation['indexes'],navigation['learning'],navigation['history']]}|new
    for path in touched:
        if path in new:
            current[path]=layout.index_text(path,headers[path],tables[path]);continue
        raw=files[path];meta,body=read(raw,allowed=set(bank.SPEC['properties'])|set(layout.SPEC['properties']))
        prefix,tail=body.split('\n## Routes\n',1)
        if path=='MEMORY.md':
            _,omitted_tail=tail.split('\n## Omitted contexts\n',1)
            meta['router-updated']=stamp
            body=prefix+'\n## Routes\n\n'+bank.route_table(tables[path],path)+'\n## Omitted contexts\n'+omitted_tail
        else:
            meta['updated']=stamp;body=prefix+'\n## Routes\n\n'+bank.route_table(tables[path],path)
        current[path]=write(meta)+body
    # Payloads are supplied by the existing governed admission builder. Validate
    # the completed candidate there, after its actual record/event are inserted.
    return current
