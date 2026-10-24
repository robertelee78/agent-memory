"""Interrupt before every observed fresh copy-install logical write."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class InstallCopyBeforeContract(LifecycleWriteCases, unittest.TestCase):
    def test_before_every_copy_install_write(self):
        self.replay('install', 'copy', 'before')
