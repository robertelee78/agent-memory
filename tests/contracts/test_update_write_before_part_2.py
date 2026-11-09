"""Replay partitions 2 and 6 of all observed lifecycle interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteBeforePart2Contract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_2(self):
        self.replay('before', partition=2)

    def test_actual_logical_writes_in_partition_6(self):
        self.replay('before', partition=6)
