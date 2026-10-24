"""Installed composition of existing substantive public memory scenarios."""
import unittest
from unittest.mock import patch

import installed_cli_fixtures as installed
from contracts import test_admission as admission
from contracts import test_assembly as assembly
from contracts import test_consolidation as consolidation
from contracts import test_governance as governance


class UnifiedCLIOperations(unittest.TestCase):
    def run_installed(self, fixture, scenario, *, governance_group=None):
        clis = {}
        def call(world, helper, *args):
            if world.root not in clis:
                clis[world.root] = installed.install(world)
            cli = clis[world.root]
            if helper == 'govern_memory.py' and governance_group:
                return installed.command(world, cli, governance_group, *args)
            return installed.invoke(world, cli, helper, *args)
        fixture.call = call
        with patch.object(admission.AdmissionContract, 'call', lambda _self, *args: call(*args)):
            getattr(fixture, scenario)()

    def test_installed_compaction_preserves_distinct_provenance_history_and_converges(self):
        self.run_installed(consolidation.ConsolidationContract(),
            'test_duplicates_keep_stable_identity_all_provenance_and_correction_history_then_converge')

    def test_installed_correction_preserves_identity_origin_history_and_repeat(self):
        self.run_installed(governance.GovernanceContract(),
            'test_stronger_correction_keeps_stable_identity_old_origin_and_useful_history', governance_group='correct')

    def test_installed_context_requires_an_actual_current_text_bound_token_count(self):
        self.run_installed(assembly.AssemblyContract(),
            'test_harness_counter_is_bound_to_exact_current_text_and_token_cap')


if __name__ == '__main__':
    unittest.main()
