"""Replay partition 0/4 of all observed update-write-after interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteAfterContract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_0(self):
        self.replay('after', partition=0)
