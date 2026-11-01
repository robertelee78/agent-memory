"""Canonical capacity failures contain bounded metadata, never memory bodies."""
from pathlib import PurePosixPath

from . import sharing_limits as limits
from .frontmatter import ContractError

REPAIR = (
    'Preserve the bank, feature work, original index/HEAD and all recovery evidence. '
    'Ordinary recall and new capture remain pending. This runtime has no supported '
    'source-preserving layout-recovery command. Do not truncate, delete, skip a '
    'source, raise normal limits or initialize over existing memory; repeating the '
    'same operation does not repair capacity.'
)
DIMENSIONS = {
    'bank-source-byte-capacity': ('source-byte', 'bytes'),
    'bank-aggregate-byte-capacity': ('aggregate-byte', 'bytes'),
    'bank-source-count-capacity': ('source-count', 'sources'),
}


class BankCapacityError(ContractError):
    """A measured resource refusal, not proof of otherwise valid bank content."""

    def __init__(self, code, path, limit, observed, *, measurement='at-least'):
        relative = PurePosixPath(path) if isinstance(path, str) else None
        if (code not in DIMENSIONS or relative is None or not path
                or relative.is_absolute() or '..' in relative.parts
                or str(relative) != path or any(ord(c) < 32 or ord(c) == 127 for c in path)
                or type(limit) is not int or type(observed) is not int
                or limit < 0 or observed <= limit or measurement not in {'exact', 'at-least'}):
            raise ContractError('Invalid canonical capacity observation; preserve sources')
        dimension, self.unit = DIMENSIONS[code]
        super().__init__('Canonical bank ' + dimension + ' capacity exceeded: '
                         + measurement + ' ' + str(observed) + '; limit ' + str(limit) + '.')
        self.code, self.path = code, path
        self.limit, self.observed, self.measurement = limit, observed, measurement
        self.repair = REPAIR

    def details(self):
        return {
            'failure_code': self.code,
            'failure_phase': 'bank-eligibility',
            'failure_component': 'canonical-bank',
            'failure_path': self.path,
            'capacity_limit': self.limit,
            'capacity_observed': self.observed,
            'capacity_measurement': self.measurement,
            'capacity_unit': self.unit,
            'repair': self.repair,
        }


def details(exc):
    """Only this known failure may cross otherwise redacted public boundaries."""
    return exc.details() if isinstance(exc, BankCapacityError) else {}


def check_files(files):
    """Check complete intended UTF-8 files; this does not validate native layout."""
    if not isinstance(files, dict):
        raise ContractError('A complete canonical file map is required')
    if len(files) > limits.MAX_FILES:
        raise BankCapacityError('bank-source-count-capacity', '.', limits.MAX_FILES,
                                len(files), measurement='exact')
    total = 0
    for path, body in files.items():
        if not isinstance(body, str):
            raise ContractError('Canonical file content must be UTF-8 text')
        amount = len(body.encode('utf-8'))
        if amount > limits.MAX_FILE:
            raise BankCapacityError('bank-source-byte-capacity', path, limits.MAX_FILE,
                                    amount, measurement='exact')
        total += amount
    if total > limits.MAX_BANK:
        raise BankCapacityError('bank-aggregate-byte-capacity', '.', limits.MAX_BANK,
                                total, measurement='exact')
