"""Replay selected registered update-write-after interruptions, or all directly."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteAfterContract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_selected_logical_writes(self):
        self.replay('after')
