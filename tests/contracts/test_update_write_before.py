"""Replay partition 0/4 of all observed update-write-before interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteBeforeContract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_0(self):
        self.replay('before', partition=0)
