"""Replay partition 2/4 of all observed update-write-before interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteBeforePart2Contract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_2(self):
        self.replay('before', partition=2)
