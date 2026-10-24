"""Replay partition 2/4 of all observed update-write-after interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteAfterPart2Contract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_2(self):
        self.replay('after', partition=2)
