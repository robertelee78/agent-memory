"""Replay partitions 3 and 7 of all observed lifecycle interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteAfterPart3Contract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_3(self):
        self.replay('after', partition=3)

    def test_actual_logical_writes_in_partition_7(self):
        self.replay('after', partition=7)
