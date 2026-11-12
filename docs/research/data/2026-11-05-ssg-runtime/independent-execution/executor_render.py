"""Byte-exact renderers for independent synthetic research; no neighbor context."""
QUERY_PREFIX = 'Instruct: Retrieve applicable saved project guidance for the question.\nQuery: '


def document(body, scope):
    if not isinstance(body, str) or not isinstance(scope, str):
        raise ValueError('Canonical body and scope must be native strings')
    return (body + '\nScope: ' + scope).encode('utf-8')


def query(text):
    if not isinstance(text, str):
        raise ValueError('Query must be the unchanged authored string')
    return (QUERY_PREFIX + text).encode('utf-8')


def identity(spec):
    expected_document = {'prefix': '', 'format': '{canonical_body}\nScope: {canonical_scope}',
                         'version': 1, 'neighbor_bodies': False}
    expected_query = {'prefix': QUERY_PREFIX, 'format': '{prefix}{authored_query}', 'version': 1}
    if spec['profile']['document_renderer'] != expected_document or spec['profile']['query_renderer'] != expected_query:
        raise ValueError('Renderer contract changed; review required before outputs')
    return {'document': expected_document, 'query': expected_query, 'encoding': 'UTF-8',
            'canonical_bytes': 'No normalization, stripping, quoting, or newline removal'}
