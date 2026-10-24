"""Replay partition 3/4 of all observed update-write-after interruptions."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases


class UpdateWriteAfterPart3Contract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_3(self):
        self.replay('after', partition=3)
