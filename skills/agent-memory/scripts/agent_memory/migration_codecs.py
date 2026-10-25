"""Production validators; no older or newer bank format is shipped here."""
from . import bank_contract
from .developer_identity import digest, encoded


def native(files):
    checked = bank_contract.validate_files(files)
    # The native preservation projection binds every canonical byte. A future
    # reviewed codec must produce the equivalent projection for its own format.
    return {'project_id': checked['project_id'], 'preservation_sha256': digest(encoded(files))}
