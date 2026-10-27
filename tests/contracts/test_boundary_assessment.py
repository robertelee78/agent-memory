"""No-learning records agent judgments, not semantic proof."""
import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[2]/'skills/agent-memory/scripts'))
from agent_memory.boundary_assessment import validate
from agent_memory.frontmatter import ContractError

class BoundaryAssessmentContract(unittest.TestCase):
    def test_duplicate_needs_explicit_read_evidence_and_rationale(self):
        row={'claim':'Production retries are three','disposition':'already-recorded',
             'reason':'The existing record covers the same production condition',
             'memory_evidence':'memory-bank/topics/retries.md#production'}
        assessment={'source_id':'turn','source_review':'Read the original turn and compared current memory',
                    'candidates':[row]}
        self.assertEqual(validate(assessment,'turn'), assessment)
        for changed in ({k:v for k,v in row.items() if k!='memory_evidence'},
                        {**row,'memory_evidence':''}, {**row,'disposition':'eligible'},
                        {**row,'disposition':'ambiguous'}):
            with self.subTest(changed=changed), self.assertRaises(ContractError):
                validate({**assessment,'candidates':[changed]},'turn')
        with self.assertRaises(ContractError): validate(assessment,'another-turn')
