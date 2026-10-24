"""Actual installed public bank operations, sharing and readable manual fallback.

Fixture source setup may construct an existing bank. All operations under test
run the installed wrapper; missing routes are failures, never source fallback.
"""
import unittest
from unittest.mock import patch

from world import World
import installed_cli_fixtures as installed
from contracts import test_initialization as initialization
from contracts import test_checkpoint as checkpoint
from contracts import test_retrieval as retrieval
from contracts import test_governance as governance
from contracts import test_comparison as comparison
from contracts import test_admission as admission
from contracts import test_shared_learning_loop as loop




class InstalledSharedLearningLoop(loop.SharedLearningLoopContract):
    def fixture(self, w):
        result = super().fixture(w)
        self.cli = installed.install(w)
        return result

    def call(self, w, helper, *args):
        return installed.invoke(w, self.cli, helper, *args)

    def setUp(self):
        # The loop's independent admission fixture must also use the installed
        # public surface rather than silently calling its copied source pack.
        outer = self
        def admission_command(_fixture, w, helper, *args):
            return outer.call(w, helper, *args)
        self.capture_patch = patch.object(admission.AdmissionContract, 'call', admission_command)
        self.capture_patch.start()
        self.addCleanup(self.capture_patch.stop)


if __name__ == '__main__':
    unittest.main()
