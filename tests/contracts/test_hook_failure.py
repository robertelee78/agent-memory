"""Private hook observations identify failures without claiming publication."""
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
import uuid
from unittest.mock import patch
from world import World
from contracts import test_admission
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'skills/agent-memory/scripts'))
from agent_memory import hook_failure

class HookFailureContract(unittest.TestCase):
    def test_independent_adapters_worktrees_private_bounds_and_readonly_absence(self):
        with World() as w:
            repo=w.repository();worktree=w.inside('session-two')
            (repo/'a').write_text('a');w.git('add','a',cwd=repo);w.git('commit','-m','seed',cwd=repo)
            w.git('worktree','add','-b','session-two',str(worktree),cwd=repo)
            with patch.dict(os.environ,w.env,clear=True):
                directory,name=hook_failure.paths(repo,'codex','prompt')
                self.assertEqual(hook_failure.read(repo),[]);self.assertFalse(directory.exists())
                hook_failure.record(repo,'codex','prompt','identity','identity.unresolved')
                hook_failure.record(repo,'codex','stop','boundary','boundary.missing')
                self.assertEqual(hook_failure.read(worktree),[])
                self.assertEqual(len(hook_failure.read(repo)),2)
                self.assertEqual((directory/name).stat().st_mode&0o777,0o600)
                self.assertEqual(directory.stat().st_mode&0o777,0o700)
                self.assertLess((directory/name).stat().st_size,4096)
                hook_failure.clear(worktree,'codex','prompt')
                self.assertEqual(len(hook_failure.read(repo)),2)
                hook_failure.clear(repo,'codex','prompt')
                self.assertEqual([v['adapter'] for v in hook_failure.read(repo)],['stop'])
                with self.assertRaises(ValueError):hook_failure.record(repo,'codex','stop','identity','sensitive arbitrary exception')
                target=directory/name;target.symlink_to(w.inside('foreign'))
                with self.assertRaises(OSError):hook_failure.record(repo,'codex','prompt','identity','identity.unresolved')
                self.assertTrue(target.is_symlink())

    def test_hook_failure_codes_survive_without_source_content(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            def run(adapter,value):
                result=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks'/('codex_'+adapter+'.py'))],input=json.dumps(value),text=True,capture_output=True,env=w.env,timeout=10)
                self.assertEqual(result.returncode,0,result.stderr);return json.loads(result.stdout)
            session,turn=str(uuid.uuid4()),str(uuid.uuid4())
            event={'hook_event_name':'Stop','session_id':session,'turn_id':turn,'stop_hook_active':False,'cwd':str(repo),'last_assistant_message':'SECRET FINAL MUST NOT BE RETAINED'}
            self.assertIn('event/event.invalid',run('stop',{})['systemMessage'])
            self.assertIn('boundary/boundary.missing',run('stop',event)['systemMessage'])
            with patch.dict(os.environ,w.env,clear=True):
                values=hook_failure.read(repo);self.assertEqual(values[0]['reason'],'boundary.missing')
                self.assertNotIn('SECRET',json.dumps(values))
            prompt={**event,'hook_event_name':'UserPromptSubmit','prompt':'SECRET PROMPT MUST NOT BE RETAINED'}
            self.assertIn('hookSpecificOutput',run('prompt',prompt))
            with patch.dict(os.environ,w.env,clear=True):self.assertEqual(len(hook_failure.read(repo)),1)
            self.assertEqual(run('stop',event)['decision'],'block')
            with patch.dict(os.environ,w.env,clear=True):self.assertEqual(hook_failure.read(repo),[])
            changed={**prompt,'prompt':'DIFFERENT SECRET'}
            self.assertIn('boundary/boundary.source_mismatch',run('prompt',changed)['systemMessage'])
            with patch.dict(os.environ,w.env,clear=True):
                self.assertNotIn('SECRET',json.dumps(hook_failure.read(repo)))

    def test_unresolved_identity_retains_fixed_metadata_without_bank_change(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            before=helper.state(w,repo,bank)
            registry=Path(w.env['XDG_CONFIG_HOME'])/'agent-memory/projects.json'
            value=json.loads(registry.read_text());value['checkouts']={};registry.write_text(json.dumps(value))
            event={'hook_event_name':'UserPromptSubmit','session_id':str(uuid.uuid4()),'turn_id':str(uuid.uuid4()),'cwd':str(repo),'prompt':'SECRET PROMPT'}
            result=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/codex_prompt.py')],input=json.dumps(event),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertIn('identity/identity.unresolved',json.loads(result.stdout)['systemMessage'])
            with patch.dict(os.environ,w.env,clear=True):
                records=hook_failure.read(repo)
                self.assertEqual(records[0]['stage'],'identity');self.assertNotIn('SECRET',json.dumps(records))
            self.assertEqual(before,helper.state(w,repo,bank))

    def test_unsafe_or_oversized_metadata_is_preserved_and_refused(self):
        with World() as w:
            repo=w.repository()
            with patch.dict(os.environ,w.env,clear=True):
                hook_failure.record(repo,'codex','prompt','identity','identity.unresolved')
                directory,name=hook_failure.paths(repo,'codex','prompt');path=directory/name
                path.write_bytes(b'X'*4097)
                with self.assertRaises(ValueError):hook_failure.read(repo)
                self.assertTrue(hook_failure.clear(repo,'codex','prompt'))
                self.assertFalse(path.exists())
                hook_failure.record(repo,'codex','prompt','identity','identity.unresolved')
                path.write_text('{}')
                hook_failure.record(repo,'codex','prompt','boundary','boundary.missing')
                self.assertEqual(hook_failure.read(repo)[0]['reason'],'boundary.missing')
                path.chmod(0o644)
                with self.assertRaises(ValueError):hook_failure.read(repo)

    def test_unexpected_exception_is_fixed_and_does_not_expose_exception_text(self):
        import importlib.util
        import io
        from contextlib import redirect_stdout
        from agent_memory import scope_binding
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            event={'hook_event_name':'Stop','session_id':str(uuid.uuid4()),'turn_id':str(uuid.uuid4()),'cwd':str(repo),'stop_hook_active':False,'last_assistant_message':'answer','prompt':'actual prompt'}
            for adapter in ('prompt','stop'):
                event['hook_event_name']='UserPromptSubmit' if adapter=='prompt' else 'Stop'
                spec=importlib.util.spec_from_file_location('hook_'+adapter,helper.pack/'hooks'/('codex_'+adapter+'.py'))
                module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
                output=io.StringIO()
                with patch.dict(os.environ,w.env,clear=True), patch.object(sys,'stdin',io.TextIOWrapper(io.BytesIO(json.dumps(event).encode()))), patch.object(scope_binding,'require',side_effect=RuntimeError('SECRET EXCEPTION CONTENT')),redirect_stdout(output):
                    self.assertEqual(module.main(),0)
                self.assertIn('identity/internal.unexpected',output.getvalue());self.assertNotIn('SECRET',output.getvalue())
            with patch.dict(os.environ,w.env,clear=True):
                records=hook_failure.read(repo);self.assertEqual(len(records),2)
                self.assertTrue(all(r['reason']=='internal.unexpected' for r in records))

    def test_malformed_fields_are_refused_and_missing_clear_writes_nothing(self):
        with World() as w:
            repo=w.repository()
            with patch.dict(os.environ,w.env,clear=True):
                directory,name=hook_failure.paths(repo,'codex','prompt')
                self.assertFalse(hook_failure.clear(repo,'codex','prompt'));self.assertFalse(directory.exists())
                hook_failure.record(repo,'codex','stop','identity','identity.unresolved')
                before=set(directory.iterdir())
                self.assertFalse(hook_failure.clear(repo,'codex','prompt'))
                self.assertEqual(set(directory.iterdir()),before)
                directory,name=hook_failure.paths(repo,'codex','stop');path=directory/name
                value=json.loads(path.read_text())
                for field in ['harness','adapter','stage','reason','timestamp']:
                    invalid={**value,field:[]};path.write_text(json.dumps(invalid))
                    with self.assertRaises(ValueError):hook_failure.read(repo)
                hook_failure.record(repo,'codex','stop','boundary','boundary.missing')
                self.assertEqual(hook_failure.read(repo)[0]['reason'],'boundary.missing')

    def test_non_git_file_only_diagnostics_are_not_hook_failures(self):
        with World() as w:
            repo=w.inside('file-only');repo.mkdir()
            with patch.dict(os.environ,w.env,clear=True):
                self.assertEqual(hook_failure.read(repo),[])
                with self.assertRaises(ValueError):hook_failure.record(repo,'codex','prompt','identity','identity.unresolved')

    def test_linked_worktree_rejects_state_in_main_checkout_memory(self):
        with World() as w:
            repo=w.repository();(repo/'a').write_text('a');w.git('add','a',cwd=repo);w.git('commit','-m','seed',cwd=repo)
            worktree=w.inside('linked');w.git('worktree','add','-b','linked',str(worktree),cwd=repo)
            forbidden=repo/'memory-bank'/'private-hook-state'
            with patch.dict(os.environ,{**w.env,'AGENT_MEMORY_STATE_HOME':str(forbidden)},clear=True):
                with self.assertRaises(ValueError):hook_failure.record(worktree,'codex','prompt','identity','identity.unresolved')
                self.assertFalse(forbidden.exists())

    def test_one_corrupt_adapter_does_not_hide_other_failure(self):
        with World() as w:
            repo=w.repository()
            with patch.dict(os.environ,w.env,clear=True):
                for adapter in ['prompt','stop']:hook_failure.record(repo,'codex',adapter,'identity','identity.unresolved')
                directory,name=hook_failure.paths(repo,'codex','prompt');(directory/name).write_text('{invalid')
                errors=[];records=hook_failure.read(repo,errors=errors)
                self.assertEqual([r['adapter'] for r in records],['stop'])
                self.assertEqual(errors,[{'harness':'codex','adapter':'prompt','reason':'state.unavailable'}])

    def test_bank_absence_clears_only_its_own_adapter(self):
        with World() as w:
            repo=w.repository();pack=Path(__file__).resolve().parents[2]/'skills/agent-memory'
            with patch.dict(os.environ,w.env,clear=True):
                for adapter in ['prompt','stop']:hook_failure.record(repo,'codex',adapter,'identity','identity.unresolved')
            event={'hook_event_name':'UserPromptSubmit','session_id':str(uuid.uuid4()),'turn_id':str(uuid.uuid4()),'cwd':str(repo),'prompt':'no bank'}
            result=subprocess.run([sys.executable,'-I',str(pack/'hooks/codex_prompt.py')],input=json.dumps(event),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertEqual(json.loads(result.stdout),{})
            with patch.dict(os.environ,w.env,clear=True):self.assertEqual([r['adapter'] for r in hook_failure.read(repo)],['stop'])
