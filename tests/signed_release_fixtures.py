"""Isolated TLS release service and fixture keys; no production signing material."""
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shutil
import ssl
import subprocess
import sys
import threading

from runtime_update_fixtures import ROOT, version


class SignedRelease:
    def __init__(self, world, source=None):
        self.w = world
        self.root = source if source is not None else world.repository('signed release source')
        self.key = world.inside('fixture-release-key')
        world.run(['ssh-keygen','-q','-t','ed25519','-N','','-f',self.key])
        self.public = ' '.join(Path(str(self.key)+'.pub').read_text().split()[:2])
        cert = world.inside('fixture-tls.crt'); tls_key = world.inside('fixture-tls.key')
        config = world.inside('fixture-tls.cnf')
        config.write_text('[req]\ndistinguished_name=dn\nx509_extensions=ext\nprompt=no\n[dn]\nCN=127.0.0.1\n[ext]\nsubjectAltName=IP:127.0.0.1\nbasicConstraints=critical,CA:TRUE\n')
        world.run(['openssl','req','-x509','-newkey','rsa:2048','-sha256','-nodes','-days','1',
                   '-keyout',tls_key,'-out',cert,'-config',config])
        world.env['CURL_CA_BUNDLE'] = str(cert)
        self.responses = {}; self.requests = []; owner = self
        self.build_count = 0
        self.signature_count = 0
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args): pass
            def do_GET(self):
                owner.requests.append(self.path)
                if self.path == '/api/latest' and self.headers.get('Accept') != 'application/vnd.github+json':
                    self.send_error(415); return
                authority = getattr(owner,'publisher_state',None)
                if authority is not None:
                    state = json.loads(authority.read_text())
                    if self.path == '/raw/main/install.sh':
                        result = subprocess.run([state['git'],'--git-dir',state['bare'],'show','main:install.sh'],
                                                env=owner.w.env,capture_output=True,timeout=10)
                        value = result.stdout if result.returncode == 0 else None
                    elif not state.get('release') or state['release']['draft']:
                        value = None
                    elif self.path == '/api/latest':
                        value = json.dumps({'tag_name':state['tag'],'draft':False,'prerelease':False}).encode()
                    elif self.path.startswith('/download/'+state['tag']+'/'):
                        name = self.path.rsplit('/',1)[1]
                        asset = state['assets'].get(name)
                        value = Path(asset['path']).read_bytes() if asset else None
                    else: value = None
                    if value is None: self.send_error(404); return
                    self.send_response(200); self.send_header('Content-Length',str(len(value)))
                    self.end_headers(); self.wfile.write(value); return
                value = owner.responses.get(self.path)
                if value is None:
                    self.send_error(404); return
                self.send_response(200); self.send_header('Content-Length',str(len(value)))
                self.end_headers(); self.wfile.write(value)
        self.server = ThreadingHTTPServer(('127.0.0.1',0),Handler)
        self.server.daemon_threads = True
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); context.load_cert_chain(cert,tls_key)
        self.server.socket = context.wrap_socket(self.server.socket,server_side=True)
        self.base = 'https://127.0.0.1:'+str(self.server.server_port)
        self.thread = threading.Thread(target=self.server.serve_forever,daemon=True)

    def __enter__(self):
        self.thread.start(); return self

    def __exit__(self, *args):
        self.server.shutdown(); self.server.server_close(); self.thread.join(timeout=5)
        if self.thread.is_alive(): raise RuntimeError('APPARATUS: fixture TLS server did not stop')

    def prepare(self, value='0.0.3'):
        pack = self.root/'skills/agent-memory'
        if pack.exists(): shutil.rmtree(pack)
        shutil.copytree(ROOT/'skills/agent-memory',pack)
        scripts = self.root/'scripts/distribution'
        shutil.copytree(ROOT/'scripts/distribution',scripts,dirs_exist_ok=True)
        path = pack/'scripts/agent_memory/release_distribution.py'
        text = path.read_text()
        import re
        text = re.sub(r"^PUBLIC_KEY = .*?$",'PUBLIC_KEY = '+repr(self.public),text,flags=re.MULTILINE)
        text = re.sub(r"^API = .*?$",'API = '+repr(self.base+'/api'),text,flags=re.MULTILINE)
        text = re.sub(r"^ASSETS = .*?$",'ASSETS = '+repr(self.base+'/download'),text,flags=re.MULTILINE)
        path.write_text(text)
        path = pack/'scripts/agent_memory/installation.py'
        path.write_text(path.read_text().replace("DEFAULT_REMOTE = 'https://github.com/robertelee78/agent-memory.git'",
                                               'DEFAULT_REMOTE = '+repr(str(self.root))))
        (pack/'keys/allowed-signers').write_text('release@agent-memory namespaces="agent-memory.release" '+self.public+'\n')
        version(pack,value)
        rendered = self.w.run([sys.executable,'-B',scripts/'release.py','render-bootstrap'])
        (self.root/'install.sh').write_text(rendered.stdout)
        self.w.git('add','.',cwd=self.root); self.w.git('commit','-m','Isolated signed release candidate',cwd=self.root)
        return self.build(value)

    def build(self, value):
        self.build_count += 1
        output = self.w.inside('assets-'+value+'-'+str(self.build_count))
        result = self.w.run([sys.executable,'-B',self.root/'scripts/distribution/release.py','build',
                             '--private-key',self.key,'--output',output],timeout=120)
        manifest = json.loads(result.stdout)
        for path in output.iterdir(): self.responses['/download/v'+value+'/'+path.name] = path.read_bytes()
        self.responses['/api/latest'] = json.dumps({'tag_name':'v'+value,'draft':False,'prerelease':False}).encode()
        return manifest

    def advance(self, value='0.0.4'):
        version(self.root/'skills/agent-memory',value)
        self.w.git('add','.',cwd=self.root); self.w.git('commit','-m','Next isolated release',cwd=self.root)
        return self.build(value)

    def signature(self, manifest, *, namespace='agent-memory.release', key=None):
        self.signature_count += 1
        path = self.w.inside('signed-input-'+str(self.signature_count))
        raw = (json.dumps(manifest,sort_keys=True,separators=(',',':'))+'\n').encode()
        path.write_bytes(raw)
        self.w.run(['ssh-keygen','-Y','sign','-n',namespace,'-f',key or self.key,path])
        return raw, Path(str(path)+'.sig').read_bytes()

    def install(self, *, mode='link', presets='none'):
        # Literal pipe journey through the same bounded HTTPS transport.
        return self.w.run(['bash','-o','pipefail','-c',
                          'curl -fsSL "$1" | bash -s -- --json --presets "$2" --mode "$3"',
                          'fixture',self.base+'/download/v0.0.3/install.sh',presets,mode],timeout=120,check=False)
