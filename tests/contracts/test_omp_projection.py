"""Stored native source projection must not turn gaps into coverage."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import omp_projection


class StoredProjection(unittest.TestCase):
    def entry(self, message):
        return {'type': 'message', 'id': 'native-1', 'parentId': None, 'message': message}

    def test_retains_tool_call_and_result_attribution(self):
        source = self.entry({'role': 'toolResult', 'toolCallId': 'call-1', 'toolName': 'read',
                             'content': [{'type': 'text', 'text': 'Example result'}],
                             'details': {'count': 1}})
        result = omp_projection.project(source)
        self.assertEqual(result['snapshot']['toolCallId'], 'call-1')
        self.assertEqual(result['snapshot']['details'], {'count': 1})
        self.assertNotIn('gap', result)

    def test_reasoning_and_provider_fields_not_copied(self):
        source = self.entry({'role': 'assistant', 'providerPayload': {'private': 'not copied'},
                             'content': [{'type': 'thinking', 'thinking': 'not copied'},
                                         {'type': 'text', 'text': 'Visible stored result',
                                          'textSignature': 'not copied'}]})
        result = omp_projection.project(source)
        self.assertEqual(result['snapshot']['content'], [{'type': 'text', 'text': 'Visible stored result'}])
        self.assertNotIn('not copied', str(result))

    def test_opaque_or_secret_source_never_retained(self):
        for block in ({'type': 'image', 'data': 'private'},
                      {'type': 'text', 'text': 'password=secretvalue'}):
            result = omp_projection.project(self.entry({'role': 'user', 'content': [block]}))
            self.assertIn('gap', result)
            self.assertNotIn('snapshot', result)
            self.assertNotIn('secretvalue', str(result))

    def test_tool_payload_metadata_is_not_copied(self):
        source = self.entry({'role': 'toolResult', 'content': 'result',
                             'details': {'providerPayload': {'secret': 'data'}}})
        self.assertIn('gap', omp_projection.project(source))
