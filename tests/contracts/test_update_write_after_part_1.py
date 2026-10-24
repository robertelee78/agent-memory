"""Replay partition 1/4 of all observed update-write-after interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteAfterPart1Contract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_1(self):
        self.replay('after', partition=1)
