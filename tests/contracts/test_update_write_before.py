"""Interrupt before each real logical update write, recover and retry."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases

class UpdateWriteBeforeContract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_before_every_observed_logical_durable_update_write(self):
        self.replay('before')
