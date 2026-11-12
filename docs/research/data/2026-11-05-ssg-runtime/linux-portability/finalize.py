#!/usr/bin/env python3
"""Final changed-path evidence qualification and exact owned acquisition disposal.

Never operates a service or user store. Does not remove borrowed model/image bytes.
Run only after the serial Linux and amd64 contents recipes have completed.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import shutil
import socket
import time
from discover import command, save, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--actual', type=Path, required=True)
    parser.add_argument('--contents', type=Path, required=True)
    parser.add_argument('--comparison', type=Path, required=True)
    parser.add_argument('--dispose-owned-acquisition', action='store_true', required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    actual = args.actual.resolve()
    contents = args.contents.resolve()
    if actual.parent != root or contents.parent != root:
        raise ValueError('Only this new owned data subtree may be finalized')
    report = json.loads((actual/'report.json').read_text())
    frozen = json.loads((actual/'frozen-inputs.json').read_text())
    image_freeze = json.loads((contents/'freeze.json').read_text())
    final_source = command(root, 'finalization-source', ['git','rev-parse','HEAD'])
    checks = {
        'linux_controller_matches_frozen_sha256': sha(root/'qualify_linux.py') == frozen['script_sha256'],
        'linux_config_matches_frozen_sha256': sha(root/'recipe.json') == frozen['config_sha256'],
        'image_controller_matches_frozen_sha256': sha(root/'image_contents.py') == image_freeze['script_sha256'],
        'image_bindings_match_frozen_sha256': sha(root/'discovery-20261106/image-bindings.json') == image_freeze['bindings_sha256'],
        'helper_matches_both_freezes': sha(root/'discover.py') == frozen['helper_sha256'] == image_freeze['helper_sha256'],
        'owned_preflight_absent': json.loads((actual/'preflight-absence.json').read_text())['exit'] == 1,
        'reported_baseline_container_ids_preserved': report['baseline_containers_preserved'],
    }
    cleanup = []
    for path in sorted(actual.glob('*/cleanup.json')):
        data = json.loads(path.read_text())
        exists = command(root, 'final-'+path.parent.name+'-container-absence', ['podman','container','exists',data['id']])
        cleanup.append({'path':str(path.relative_to(root)), 'sha256':sha(path), 'id':data['id'], 'receipt_absent':data['absent'], 'final_exit':exists['exit']})
        checks[path.parent.name+'_owned_container_absent'] = data['absent'] and exists['exit'] == 1
        create_argv = json.loads((path.parent/'create.json').read_text())['argv']
        publication = create_argv[create_argv.index('--publish')+1]
        port = int(publication.split(':')[1])
        with socket.socket() as sock:
            sock.settimeout(.25)
            checks[path.parent.name+'_owned_loopback_listener_absent'] = sock.connect_ex(('127.0.0.1',port)) != 0
    bindings = json.loads((root/'discovery-20261106/image-bindings.json').read_text())
    image_report = json.loads((contents/'report.json').read_text())
    for image in image_report['results']:
        if 'missing_prerequisite' in image:
            continue
        data = json.loads((contents/image['name']/'contents.json').read_text())
        expected = next(x for x in bindings[image['name']]['children'] if x['platform']['architecture']=='amd64')
        checks[image['name']+'_exact_amd64_manifest'] = data['acquired_manifest_sha256'] == expected['manifest'].split(':')[1]
        checks[image['name']+'_all_pinned_layers_verified'] = all(x['verified'] for x in data['layers'])
        checks[image['name']+'_elf_machine_amd64'] = data['memgraph_binary']['elf_machine'] == 62
    for relative in ('recipe.json','qualification-contract.json','discovery-20261106/selected-release.json'):
        json.loads((root/relative).read_text())
    # The only owned binary-byte storage is this exact, new subtree directory.
    acquired = root/'.acquired'
    if acquired.is_symlink() or acquired.resolve().parent != root:
        raise ValueError('Owned acquisition root identity refused')
    acquired_inventory = []
    for path in sorted(acquired.rglob('*')):
        if path.is_symlink():
            acquired_inventory.append({'path':str(path.relative_to(acquired)),'symlink':str(path.readlink())})
        elif path.is_file():
            acquired_inventory.append({'path':str(path.relative_to(acquired)),'bytes':path.stat().st_size,'sha256':sha(path)})
    save(root/'owned-acquisition-inventory.json', acquired_inventory)
    shutil.rmtree(acquired)
    checks['owned_acquisition_directory_absent'] = not acquired.exists()
    for profile, expected in frozen['config']['profiles'].items():
        # Borrowed weight was already hashed before use; final disposition verifies presence/size, not new provenance.
        path = Path(expected['weight'])
        checks[profile+'_borrowed_weight_preserved'] = path.is_file() and path.stat().st_size == expected['bytes']
    carrier = command(root, 'final-borrowed-carrier-presence', ['podman','image','exists',frozen['config']['carrier_image']])
    checks['borrowed_carrier_image_preserved'] = carrier['exit'] == 0
    save(root/'final-qualification.json', {'schema':'am255/linux-final-changed-path-qualification-v1',
        'finalizer_sha256':sha(__file__),'start_unix':time.time(),'checks':checks,'all_qualification_checks':all(checks.values()),
        'owned_container_disposition':cleanup,'owned_acquisition_files_disposed':len(acquired_inventory),
        'borrowed_images_weights_preserved':True,'forensic_media_provider_backup_erasure':False,
        'not_an_acceptance_pass':['legal','managed profile','G1','quality','clean performance','native amd64 support']})
    allfiles = [{'path':str(p.relative_to(root)),'bytes':p.stat().st_size,'sha256':sha(p)} for p in sorted(root.rglob('*')) if p.is_file() and '.acquired' not in p.parts and '__pycache__' not in p.parts]
    save(root/'run-index.json', {'schema':'am255/linux-portability-evidence-index-v1',
        'am_baseline':frozen['am_baseline'],'linux_executed_source':frozen['executed_source_commit'],
        'image_contents_executed_source':image_freeze['executed_source_commit'],'ssg':frozen['ssg'],
        'finalization_source_commit':final_source['stdout'].strip(),
        'upstream':frozen['config']['upstream_commit'],'actual_report':str((actual/'report.json').relative_to(root)),
        'actual_report_sha256':sha(actual/'report.json'),'image_report':str((contents/'report.json').relative_to(root)),
        'image_report_sha256':sha(contents/'report.json'),'comparison':str(args.comparison),'comparison_sha256':sha(args.comparison),
        'frozen_linux_recipe_sha256':frozen['config_sha256'],'final_qualification_sha256':sha(root/'final-qualification.json'),
        'platform_scope':'Actual Linux-arm64 CPU in Podman AppleHV VM; nonexecuting amd64 merged-layer contents only',
        'files':allfiles,'G1_acceptance':False,'legal_acceptance':False,'profile_acceptance':False,'performance_acceptance':False})
    print(json.dumps({'qualification_checks':checks,'all_qualification_checks':all(checks.values()),'run_index_sha256':sha(root/'run-index.json')},indent=2))
    if not all(checks.values()):
        raise SystemExit(1)

if __name__ == '__main__':
    main()
