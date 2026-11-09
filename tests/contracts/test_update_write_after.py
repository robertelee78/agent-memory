"""Replay partitions 0 and 4 of all observed lifecycle interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteAfterContract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_0(self):
        self.replay('after', partition=0)

    def test_actual_logical_writes_in_partition_4(self):
        self.replay('after', partition=4)
