"""Actual child ownership observation around public grouped command collection."""
import os
import sys
import unittest
from unittest.mock import patch

from world import World


class WorkerOwnershipContract(unittest.TestCase):
    def test_normal_collection_never_signals_a_group_after_reaping_its_owner(self):
        violations = []; real_killpg = os.killpg
        def observed_killpg(pid, signal):
            try:
                observed, _ = os.waitpid(pid, os.WNOHANG)
            except ChildProcessError:
                violations.append({'group':pid,'reason':'leader is no longer an owned unreaped child'})
                # The observer prevents the unsafe signal in this regression.
                raise ProcessLookupError('Observed already-reaped owner')
            if observed:
                # Observation itself reaped a zombie; never signal after that.
                raise ProcessLookupError('Observer collected the owned exited child')
            return real_killpg(pid, signal)
        with World() as world, patch.object(os, 'killpg', observed_killpg):
            result = world.run([sys.executable, '-I', '-c', 'print("owned-worker-complete")'], group=True)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout.strip(), 'owned-worker-complete')
        self.assertEqual(violations, [], violations)


if __name__ == '__main__':unittest.main()
