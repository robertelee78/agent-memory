"""Interrupt after every observed install logical write (copy)."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWriteContract(LifecycleWriteCases, unittest.TestCase):
    def test_after_every_install_copy_write(self):
        self.replay('install', 'copy', 'after')
