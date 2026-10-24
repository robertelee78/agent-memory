"""Interrupt after each real logical update write, recover and retry."""
import unittest
from runtime_update_interrupts import UpdateWriteInterruptCases

class UpdateWriteAfterContract(UpdateWriteInterruptCases, unittest.TestCase):
    def test_after_every_observed_logical_durable_update_write(self):
        self.replay('after')
