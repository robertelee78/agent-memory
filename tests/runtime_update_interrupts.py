"""Replay before/after every observed logical update write through real commands."""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from runtime_update_fixtures import ROOT,fixture,advance,call,snapshot,owned,receipt_path,bank


class UpdateWriteInterruptCases:
    def replay(self, phase):
        with World() as world:
            channel,pack,receipt=fixture(world)
            memory=bank(world,world.paths['repo']); memory_before=snapshot(memory)
            revision=advance(world,channel,pack)
            baseline=world.inside('baseline'); baseline.mkdir()
            parts={'data':world.paths['data'],'state':world.paths['state'],'home':world.paths['home'],
                   'codex':world.paths['codex'],'custom':world.inside('custom discovery with spaces')}
            for key,path in parts.items(): shutil.copytree(path,baseline/key,symlinks=True)
            previous=owned(receipt)
            trace=world.inside('clean-trace.jsonl')
            _,clean=call(world,receipt,env={'AGENT_MEMORY_FAULT_TRACE':str(trace)})
            self.assertTrue(clean['verified'])
            events=[json.loads(line) for line in trace.read_text().splitlines()]
            self.assertTrue(events)
            steps={event['step'] for event in events}
            self.assertEqual(steps,set(range(len(steps))))
            self.assertEqual(len(events),2*len(steps))
            self.assertTrue({'file-stage','record-publication','payload-snapshot','atomic-node-switch'}
                            <= {event['kind'] for event in events})
            selected=[event for event in events if event['phase']==phase]
            self.assertEqual(len(selected),len(steps))
            executed=[]
            for event in selected:
                with self.subTest(point=event['point'],kind=event['kind']):
                    for key,path in parts.items():
                        shutil.rmtree(path); shutil.copytree(baseline/key,path,symlinks=True)
                    # Only disposable candidate input, never a bank or active payload.
                    shutil.rmtree(world.paths['tmp']); world.paths['tmp'].mkdir(mode=0o700)
                    actual_trace=world.inside('actual-trace.jsonl'); actual_trace.unlink(missing_ok=True)
                    process,_=call(world,receipt,expected=86,env={
                        'AGENT_MEMORY_FAULT_STAGE':event['point'],'AGENT_MEMORY_FAULT_MODE':'exit',
                        'AGENT_MEMORY_FAULT_TRACE':str(actual_trace)})
                    observed=[json.loads(line) for line in actual_trace.read_text().splitlines()]
                    self.assertEqual(observed[-1]['point'],event['point'])
                    recovered=world.run([sys.executable,'-I',ROOT/'scripts/installer.py','recover'])
                    recovery=json.loads(recovered.stdout); self.assertTrue(recovery['verified'])
                    current=json.loads(receipt_path(world).read_text())
                    committed=current['source']['revision']==revision
                    if not committed: self.assertEqual(owned(receipt),previous)
                    self.assertEqual(snapshot(memory),memory_before)
                    _,retried=call(world,receipt)
                    self.assertTrue(retried['verified'])
                    self.assertEqual(retried['receipt']['source']['revision'],revision)
                    self.assertEqual(retried['receipt']['source']['artifact_digest'],clean['receipt']['source']['artifact_digest'])
                    self.assertEqual(snapshot(memory),memory_before)
                    self.assertEqual((world.paths['codex']/'AGENTS.md').read_text().count('<!-- agent-memory:activation:begin -->'),1)
                    row={'point':event['point'],'kind':event['kind'],'exit':process.returncode,
                         'recovery_verified':True,'committed_before_retry':committed,'retry_verified':True,
                         'source_revision':revision,'bank_unchanged':True}
                    sys.stderr.write(json.dumps(row)+'\n'); executed.append(event['point'])
            self.assertEqual(len(executed),len(selected))
            sys.stderr.write(json.dumps({'phase':phase,'observed_points':len(selected),
                                         'actual_interruptions_and_retries':len(executed)})+'\n')
