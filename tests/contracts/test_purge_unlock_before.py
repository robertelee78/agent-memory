"""Interrupted pre-unlock refusal must retain useful memory without purge consent."""
import unittest
import json
import os
import socket
import sys
import uuid
from datetime import datetime, timezone
from world import World
from lifecycle_fault_fixtures import invoke
from lifecycle_unlock_fixtures import PurgeUnlockCases
from runtime_update_fixtures import snapshot


class PurgeUnlockBeforeContract(PurgeUnlockCases, unittest.TestCase):
    def test_before_refused_purge_unlock(self):
        self.replay_unlock('before')

    def test_live_foreign_or_malformed_lock_is_not_released(self):
        for case in ('live', 'foreign-host', 'malformed-owner'):
            with self.subTest(case=case), World() as world:
                memory, argv = self.fixture(world)
                owner = {'operation_id': str(uuid.uuid4()), 'operation': 'bank-purge',
                         'host': socket.gethostname(), 'pid': os.getpid(),
                         'start': datetime.now(timezone.utc).isoformat()}
                if case == 'foreign-host':
                    owner['host'] += '-different-host'
                elif case == 'malformed-owner':
                    owner['pid'] = int(world.run([sys.executable, '-I', '-c',
                                                 'import os; print(os.getpid())']).stdout)
                    del owner['operation_id']
                (memory / '.agent-memory.lock').write_text(json.dumps(owner))
                before = snapshot(memory)
                self.assertFalse(invoke(world, argv, expected=1)['verified'])
                self.assertEqual(snapshot(memory), before)
