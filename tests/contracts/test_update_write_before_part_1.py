"""Replay partitions 1 and 5 of all observed lifecycle interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteBeforePart1Contract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_1(self):
        self.replay('before', partition=1)

    def test_actual_logical_writes_in_partition_5(self):
        self.replay('before', partition=5)
