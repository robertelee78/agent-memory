"""Pinned old-runtime recovery guards against real layout markers."""
import json
from pathlib import Path
import sys
import uuid

from runtime_update_fixtures import snapshot
import layout_recovery_fixtures as recovery


def guard(case, world, old, pack, repo, bank, directory, inspected, native, *, decoy=False):
    """Clear only the real stale lock, then exercise old recovery paths."""
    marker = bank / '.agent-memory-transaction.md'
    original_marker = marker.read_bytes() if marker.exists() else None
    if original_marker is not None:
        case.assertNotIn('transaction_id', json.loads(original_marker))
    decoy_request = None
    if decoy:
        spec = recovery.save(world, {'repo': str(repo), 'files': native,
            'project_id': inspected['manifest']['project_id'],
            'scope': inspected['manifest']['identity']['scope_context'], 'operation_id': str(uuid.uuid4())})
        code = ("import sys,json;from pathlib import Path;sys.path.insert(0,sys.argv[1]);"
                "from agent_memory import bank_transactions as tx;"
                "from agent_memory.developer_identity import encoded,digest;"
                "s=json.loads(Path(sys.argv[2]).read_bytes());files=s['files'];"
                "p={'schema':tx.SCHEMA,'operation_id':s['operation_id'],'operation':'compact',"
                "'repo_root':s['repo'],'project_id':s['project_id'],'observed_hashes':tx.file_hashes(files),"
                "'files':files,'scope_context':s['scope']};p=tx.proposal_value(p);"
                "j={'schema':'agent-memory/transaction-journal-v1','proposal':p,'base_files':files};"
                "print(json.dumps({'journal':j,'journal_sha256':digest(encoded(j)),'hashes':p['observed_hashes']}))")
        built = world.run([sys.executable, '-I', '-B', '-c', code, str(pack / 'scripts'), str(spec)],
                          check=False, timeout=30)
        case.assertEqual(built.returncode, 0, built.stdout + built.stderr)
        value = json.loads(built.stdout)
        decoy_id = json.loads(spec.read_text())['operation_id']
        journal = directory.parent.parent / 'transactions' / decoy_id / 'journal.json'
        journal.parent.mkdir(parents=True, mode=0o700)
        journal.write_bytes(recovery.encoded(value['journal'])); journal.chmod(0o600)
        request = {'schema': 'agent-memory/transaction-recovery-v1', 'operation_id': decoy_id,
                   'repo_root': str(repo), 'mode': 'rollback', 'journal_sha256': value['journal_sha256'],
                   'marker_sha256': recovery.sha(original_marker or b''), 'observed_hashes': value['hashes']}
        decoy_request = recovery.save(world, request)
    lock = bank / '.agent-memory.lock'
    case.assertTrue(lock.is_file())
    def state_without_lock():
        value = snapshot(bank)
        value.pop('.agent-memory.lock', None)
        index = repo / '.git/index'
        head = world.git('rev-parse', '--verify', '--quiet', 'HEAD', cwd=repo, check=False)
        return (value, snapshot(directory.parent.parent), index.read_bytes() if index.exists() else None,
                (head.returncode, head.stdout), snapshot(repo / 'feature.txt'), snapshot(repo / 'untracked.txt'))
    before = state_without_lock()
    preview = world.run([old, 'writes', 'recover', '--repo', repo], check=False, timeout=60)
    case.assertEqual(preview.returncode, 0, preview.stdout + preview.stderr)
    unlocked = world.run([old, 'writes', 'recover', '--repo', repo,
                          '--proposal', recovery.save(world, json.loads(preview.stdout))], check=False, timeout=60)
    case.assertEqual(unlocked.returncode, 0, unlocked.stdout + unlocked.stderr)
    case.assertFalse(lock.exists())
    case.assertEqual(state_without_lock(), before)
    operations = ([['transaction', 'recover', '--repo', repo, '--mode', mode] for mode in ('finish', 'rollback')]
                  + [['migrate', 'recover', '--repo', repo, '--mode', mode] for mode in ('finish', 'rollback')])
    if decoy_request:
        operations.append(['transaction', 'recover', '--repo', repo, '--proposal', decoy_request])
    for args in operations:
        refused = world.run([old, *args], check=False, timeout=60)
        case.assertNotEqual(refused.returncode, 0, refused.stdout + refused.stderr)
        if decoy_request and '--proposal' in args:
            case.assertIn('marker', json.loads(refused.stdout)['error'].lower())
        case.assertEqual(state_without_lock(), before)
        case.assertEqual(marker.read_bytes() if marker.exists() else None, original_marker)
    doctor = world.run([old, 'doctor', '--fix', '--repo', repo, '--json'], check=False, timeout=60)
    case.assertFalse(json.loads(doctor.stdout)['repairs_performed'])
    case.assertEqual(state_without_lock(), before)
    case.assertEqual(marker.read_bytes() if marker.exists() else None, original_marker)
