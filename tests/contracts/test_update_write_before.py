"""Replay selected registered update-write-before interruptions, or all directly."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteBeforeContract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_selected_logical_writes(self):
        self.replay('before')
