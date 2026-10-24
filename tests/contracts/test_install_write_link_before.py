"""Interrupt before every observed install logical write (link)."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWriteContract(LifecycleWriteCases, unittest.TestCase):
    def test_before_every_install_link_write(self):
        self.replay('install', 'link', 'before')
