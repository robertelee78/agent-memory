"""Real signed one-line installation and installed lifecycle on an isolated TLS service."""
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import unittest

from world import World
from runtime_update_fixtures import ROOT, snapshot, receipt_path, owned
from signed_release_fixtures import SignedRelease


class SignedReleaseLifecycle(unittest.TestCase):
    def value(self, w, cli, *args, expected=0, env=None):
        result = w.run([cli,'--json',*args],check=False,timeout=120,env=env)
        self.assertEqual(result.returncode,expected,(result.stdout+result.stderr)[:2500])
        return json.loads(result.stdout)

    def test_current_signed_update_checks_manifest_without_archive_or_bank_scan(self):
        with World() as w, SignedRelease(w) as release:
            release.prepare()
            installed = json.loads(release.install().stdout)
            self.assertTrue(installed['verified'])
            cli = Path(installed['receipt']['bin_dir'])/'am'
            release.requests.clear()
            value = self.value(w, cli, 'update')
            self.assertTrue(value['verified'])
            self.assertFalse(value['changed'])
            self.assertEqual(value['compatibility']['bank_scan'], 'not-needed-no-changes')
            self.assertFalse(any(p.endswith('/agent-memory.tar.gz') for p in release.requests), release.requests)
            self.assertTrue(any(p.endswith('/release-manifest.json.sshsig') for p in release.requests), release.requests)
            before = receipt_path(w).read_bytes()
            signature = '/download/v0.0.3/release-manifest.json.sshsig'
            raw = bytearray(release.responses[signature]); raw[len(raw)//2] ^= 1
            release.responses[signature] = bytes(raw)
            rejected = self.value(w, cli, 'update', expected=1)
            self.assertFalse(rejected['verified'])
            self.assertEqual(receipt_path(w).read_bytes(), before)

    def test_actual_v002_index_refresh_and_deleted_worktree_upgrade_through_signed_one_liner(self):
        import shutil
        from identity_fixtures import bind
        from runtime_update_fixtures import bank
        with World() as w:
            channel = w.repository('old official runtime')
            frozen = ROOT/'tests/fixtures/v0.0.2-runtime/portable-pack.tar.gz'
            proof = json.loads((frozen.parent/'provenance.json').read_text())
            self.assertEqual(hashlib.sha256(frozen.read_bytes()).hexdigest(), proof['sha256'])
            with tarfile.open(frozen) as archive:
                for member in archive:
                    relative = Path(member.name)
                    self.assertFalse(relative.is_absolute()); self.assertNotIn('..', relative.parts)
                    path = channel/relative
                    if member.isdir(): path.mkdir(parents=True, exist_ok=True)
                    else:
                        self.assertTrue(member.isfile()); path.parent.mkdir(parents=True, exist_ok=True)
                        path.write_bytes(archive.extractfile(member).read())
                        path.chmod(0o755 if member.mode & 0o111 else 0o644)
            w.git('add', '.', cwd=channel); w.git('commit', '-m', 'Released old runtime', cwd=channel)
            old = w.run(['bash', '-c', 'umask 022; exec "$@"', 'fixture', sys.executable, '-I',
                         channel/'skills/agent-memory/scripts/installer.py', 'install', '--remote', channel,
                         '--ref', 'main', '--presets', 'none'])
            receipt = json.loads(old.stdout)['receipt']; source = Path(receipt['source']['path'])
            self.assertEqual(receipt['source']['artifact_digest'], proof['checkout_artifact_digest'])
            old_index = (source/'.git/index').read_bytes()
            self.assertEqual(w.git('status', '--porcelain', cwd=source).stdout, '')
            self.assertNotEqual((source/'.git/index').read_bytes(), old_index)
            prior_source = snapshot(source)
            repo = w.repository('registered consumer'); memory = bank(w, repo); bind(w, repo)
            w.git('add', '.', cwd=repo); w.git('commit', '-m', 'Consumer bank', cwd=repo)
            missing = w.inside('removed consumer session')
            w.git('worktree', 'add', '-b', 'removed-session', missing, cwd=repo); shutil.rmtree(missing)
            before = snapshot(repo)
            with SignedRelease(w, channel) as release:
                release.prepare()
                installed = release.install()
                self.assertEqual(installed.returncode, 0, installed.stdout + installed.stderr)
                result = json.loads(installed.stdout); self.assertTrue(result['verified'])
                adopted = result['receipt']; self.assertEqual(adopted['source']['kind'], 'release')
                self.assertFalse((source/'.git').exists()); self.assertFalse((source/'memory-bank').exists())
                backups = [Path(b['path']) for b in adopted['backups'] if b['original_path'] == str(source)
                           and b.get('kind') == 'pre-activation-snapshot']
                self.assertTrue(backups); self.assertEqual(snapshot(backups[-1]), prior_source)
                cli = Path(adopted['bin_dir'])/'am'
                version = self.value(w, cli, 'version')
                self.assertEqual(version['security']['publisher_authentication'], 'verified')
                self.assertFalse(self.value(w, cli, 'update')['changed'])
                self.assertEqual(snapshot(repo), before); self.assertTrue(memory.is_dir())

    def test_one_line_install_update_failures_recovery_and_preservation(self):
        with World() as w, SignedRelease(w) as release:
            release.prepare()
            config = w.paths['codex']/'AGENTS.md'; config.write_text('Keep my instructions.\n')
            installed = release.install(mode='copy',presets='codex')
            self.assertEqual(installed.returncode,0,installed.stdout+installed.stderr)
            receipt = json.loads(installed.stdout)['receipt']; self.assertEqual(receipt['source']['kind'],'release')
            source = Path(receipt['source']['path']); cli = Path(receipt['bin_dir'])/'am'
            self.assertFalse((source/'.git').exists()); self.assertFalse((source/'memory-bank').exists())
            version = self.value(w,cli,'version'); self.assertEqual(version['security']['publisher_authentication'],'verified')
            self.assertTrue(version['security']['signature_verification_performed'])
            self.assertFalse(version['security']['executing_verifier_authenticated'])
            before = (snapshot(source),receipt_path(w).read_bytes())
            before_targets = owned(receipt)
            checked = self.value(w,cli,'update','--check'); self.assertFalse(checked['update_available'])
            self.value(w,cli,'update')
            self.assertEqual((snapshot(source),receipt_path(w).read_bytes()),before)
            manifest = release.advance()
            good = dict(release.responses)
            for path in ('release-manifest.json','release-manifest.json.sshsig','agent-memory.tar.gz'):
                url = '/download/v0.0.4/'+path
                corrupt = bytearray(good[url]); corrupt[len(corrupt)//2] ^= 1
                release.responses[url] = bytes(corrupt)
                rejected = self.value(w,cli,'update',expected=1)
                self.assertFalse(rejected['verified'])
                self.assertEqual((snapshot(source),receipt_path(w).read_bytes()),before)
                self.assertEqual(owned(receipt),before_targets)
                release.responses = dict(good)
            missing = '/download/v0.0.4/release-manifest.json.sshsig'
            release.responses.pop(missing)
            self.value(w,cli,'update',expected=1)
            self.assertEqual(owned(receipt),before_targets)
            release.responses = dict(good)
            # Valid signatures with the wrong key/namespace are actual crypto
            # failures, rather than harmless trailing signature armor bytes.
            wrong_key = w.inside('untrusted-release-key')
            w.run(['ssh-keygen','-q','-t','ed25519','-N','','-f',wrong_key])
            for kwargs in ({'namespace':'another.release'},{'key':wrong_key}):
                raw, signature = release.signature(manifest,**kwargs)
                release.responses['/download/v0.0.4/release-manifest.json'] = raw
                release.responses[missing] = signature
                self.value(w,cli,'update',expected=1)
                self.assertEqual(owned(receipt),before_targets)
            release.responses = dict(good)
            # A publisher-signed unsafe archive must still never extract links
            # or traversal paths, nor reach candidate code.
            for name, kind in [('skills/agent-memory/escape',tarfile.SYMTYPE),
                               ('skills/agent-memory/../../escape',tarfile.REGTYPE)]:
                blob = io.BytesIO()
                with tarfile.open(fileobj=blob,mode='w:gz') as archive:
                    member = tarfile.TarInfo(name); member.type = kind; member.mode = 0o644
                    if kind == tarfile.SYMTYPE: member.linkname = '/outside'
                    archive.addfile(member)
                archive_raw = blob.getvalue(); bad = dict(manifest)
                bad.update(archive_size=len(archive_raw),archive_sha256=hashlib.sha256(archive_raw).hexdigest())
                raw, signature = release.signature(bad)
                release.responses['/download/v0.0.4/release-manifest.json'] = raw
                release.responses[missing] = signature
                release.responses['/download/v0.0.4/agent-memory.tar.gz'] = archive_raw
                self.value(w,cli,'update',expected=1)
                self.assertEqual(owned(receipt),before_targets)
            release.responses = dict(good)
            release.advance('0.0.2')
            self.value(w,cli,'update',expected=1)
            self.assertEqual(owned(receipt),before_targets)
            extra = release.root/'skills/agent-memory/templates/replacement.txt'; extra.write_text('Changed signed version.\n')
            release.advance('0.0.3')
            self.value(w,cli,'update',expected=1)
            self.assertEqual(owned(receipt),before_targets)
            release.responses = dict(good)
            # Real process loss after source activation, then trusted recovery
            # from retained verified v0.0.3 source, not a second state machine.
            interrupted = w.run([cli,'--json','update'],check=False,timeout=120,env={
                'AGENT_MEMORY_FAULT_STAGE':'update-after-activate-0','AGENT_MEMORY_FAULT_MODE':'exit'})
            self.assertEqual(interrupted.returncode,86,interrupted.stdout+interrupted.stderr)
            backup = next(p for p in source.parent.iterdir() if p.name.startswith('.agent-memory-backup-'))
            recovered = w.run([sys.executable,'-I',backup/'skills/agent-memory/scripts/installer.py','recover'],check=False)
            self.assertEqual(recovered.returncode,0,recovered.stdout+recovered.stderr)
            self.assertEqual(snapshot(source),before[0])
            self.value(w,cli,'update')
            self.assertEqual(self.value(w,cli,'version')['executing_source']['version'],'0.0.4')
            bank = w.paths['repo']/'memory-bank'; bank.mkdir(); (bank/'keep.md').write_text('Consumer project memory stays.\n')
            preserved = snapshot(bank)
            self.value(w,cli,'uninstall')
            self.assertEqual(snapshot(bank),preserved); self.assertFalse(cli.exists())
            self.assertEqual(config.read_text(),'Keep my instructions.\n')
            release.responses['/api/latest'] = json.dumps({'tag_name':'v0.0.3','draft':False,'prerelease':False}).encode()
            reinstalled = release.install(); self.assertEqual(reinstalled.returncode,0,reinstalled.stdout+reinstalled.stderr)
            self.assertEqual(snapshot(bank),preserved)

    def test_missing_signature_verifier_refuses_before_lifecycle_state_is_created(self):
        with World() as w, SignedRelease(w) as release:
            release.prepare()
            failing = w.inside('old-openssh'); failing.mkdir()
            executable = failing/'ssh-keygen'; executable.write_text('#!/bin/sh\nexit 1\n'); executable.chmod(0o755)
            w.env['PATH'] = str(failing)+':'+w.env['PATH']
            before = (snapshot(w.paths['state']),snapshot(w.paths['data']))
            rejected = release.install()
            self.assertEqual(rejected.returncode,1,rejected.stdout+rejected.stderr)
            self.assertEqual((snapshot(w.paths['state']),snapshot(w.paths['data'])),before)

    def test_actual_v002_process_then_same_version_signed_adoption_under_022_and_002(self):
        for mask in ('022','002'):
            with self.subTest(umask=mask), World() as w:
                channel = w.repository('legacy official channel')
                frozen = ROOT/'tests/fixtures/v0.0.2-runtime/portable-pack.tar.gz'
                proof = json.loads((frozen.parent/'provenance.json').read_text())
                self.assertEqual(hashlib.sha256(frozen.read_bytes()).hexdigest(),proof['sha256'])
                with tarfile.open(frozen) as archive:
                    for member in archive:
                        path = channel/member.name
                        self.assertFalse(Path(member.name).is_absolute()); self.assertNotIn('..',Path(member.name).parts)
                        if member.isdir(): path.mkdir(parents=True,exist_ok=True)
                        else:
                            self.assertTrue(member.isfile()); path.parent.mkdir(parents=True,exist_ok=True)
                            path.write_bytes(archive.extractfile(member).read()); path.chmod(member.mode)
                w.git('add','.',cwd=channel); w.git('commit','-m','Actual released v0.0.2 bytes',cwd=channel)
                old = w.run(['bash','-c','umask "$1"; shift; exec "$@"','fixture',mask,
                             sys.executable,'-I',channel/'skills/agent-memory/scripts/am_console.py','install',
                             '--json','--remote',channel,'--ref','main','--presets','none'],timeout=120)
                old_receipt = json.loads(old.stdout)['receipt']; cli = Path(old_receipt['bin_dir'])/'am'
                with SignedRelease(w,channel) as release:
                    release.prepare()
                    (channel/'memory-bank').mkdir(); (channel/'memory-bank/development.md').write_text('Repository development memory.\n')
                    w.git('add','.',cwd=channel); w.git('commit','-m','Later documentation and development memory',cwd=channel)
                    updated = w.run(['bash','-c','umask "$1"; shift; exec "$@"','fixture',mask,cli,'--json','update'],timeout=120)
                    transitional = json.loads(updated.stdout)['receipt']
                    self.assertEqual(transitional['source']['kind'],'managed')
                    observed = self.value(w,cli,'version')
                    self.assertFalse(observed['security']['signature_verification_performed'])
                    adopted = self.value(w,cli,'update')['receipt']
                    self.assertEqual(adopted['source']['kind'],'release')
                    self.assertNotEqual(adopted['source']['revision'],transitional['source']['revision'])
                    self.assertFalse((Path(adopted['source']['path'])/'memory-bank').exists())
                    self.assertTrue(self.value(w,cli,'version')['security']['signature_verification_performed'])
