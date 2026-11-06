#!/usr/bin/env python3
"""Derive architecture/BOM differences from retained reports; no service or network."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
from discover import save, sha


def differences(arm, amd):
    arm_names = {x['name']: x['version'] for x in arm}
    amd_names = {x['name']: x['version'] for x in amd}
    return {'only_arm64': {k:arm_names[k] for k in sorted(arm_names.keys()-amd_names.keys())},
            'only_amd64': {k:amd_names[k] for k in sorted(amd_names.keys()-arm_names.keys())},
            'different_versions': {k:{'arm64':arm_names[k],'amd64':amd_names[k]} for k in sorted(arm_names.keys()&amd_names.keys()) if arm_names[k]!=amd_names[k]},
            'same_name_version_count': sum(arm_names[k]==amd_names[k] for k in arm_names.keys()&amd_names.keys())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--contents', type=Path, required=True)
    parser.add_argument('--arm64', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    results = {}
    for name in ('core','mage'):
        arm_path = args.arm64 / (name+'-bom.json')
        amd_path = args.contents / name / 'contents.json'
        if not amd_path.exists():
            results[name] = {'missing_prerequisite':'No actual exact amd64 contents report; not inherited from arm64'}
            continue
        arm = json.loads(arm_path.read_text())
        amd = json.loads(amd_path.read_text())
        arm_packages = [{'name':x['Package'],'version':x['Version']} for x in arm['dpkg'] if x['Status']=='install ok installed']
        amd_packages = [x for x in amd['packages'] if x['status']=='install ok installed']
        arm_modules = [x for x in arm['memgraph_lib'] if x['path'].startswith('/usr/lib/memgraph/query_modules/')]
        amd_modules = amd['query_modules']
        arm_module_names = {x['path'].removeprefix('/') for x in arm_modules}
        amd_module_names = {x['path'] for x in amd_modules}
        arm_legal = {x['path'].removeprefix('/'):x['sha256'] for x in arm['share_doc_files'] if x['path'].startswith('/usr/share/doc/memgraph/')}
        amd_legal = {x['path']:x.get('sha256') for x in amd['legal_files']}
        results[name] = {'arm64_source':{'path':str(arm_path),'sha256':sha(arm_path),'class':'prior observed pinned arm64 inventory'},
            'amd64_source':{'path':str(amd_path),'sha256':sha(amd_path),'class':'new exact merged layer contents; nonexecuting'},
            'amd64_identity':amd['binding'], 'amd64_regular_file_bytes':amd['regular_file_bytes'],
            'installed_packages':{'arm64':len(arm_packages),'amd64':len(amd_packages),'differences':differences(arm_packages,amd_packages)},
            'python':{'arm64':len(arm['python_dists']),'amd64':len(amd['python_distributions']), 'differences':differences(arm['python_dists'],amd['python_distributions'])},
            'nested_vendor_python_metadata':{'amd64_count':len(amd['vendored_python_metadata']),
                'amd64_records':[{'name':x['name'],'version':x['version'],'path':x['metadata']['path']} for x in amd['vendored_python_metadata']],
                'scope':'Additional nested metadata surface; prior arm64 importlib inventory did not enumerate it, so not evidence of architecture differences'},
            'query_modules':{'arm64':len(arm_modules),'amd64':len(amd_modules),'only_arm64':sorted(arm_module_names-amd_module_names),'only_amd64':sorted(amd_module_names-arm_module_names)},
            'memgraph_legal':{'arm64_count':len(arm_legal),'amd64_count':len(amd_legal),'identical_path_and_bytes':arm_legal==amd_legal,
                'different':{k:{'arm64':arm_legal.get(k),'amd64':amd_legal.get(k)} for k in sorted(arm_legal.keys()|amd_legal.keys()) if arm_legal.get(k)!=amd_legal.get(k)}},
            'amd64_memgraph_binary':amd['memgraph_binary'],'amd64_mgconsole':amd['mgconsole'],
            'amd64_packages_without_copyright':[{k:x[k] for k in ('name','version','maintainer')} for x in amd_packages if not x['copyright']],
            'amd64_python_without_license_in_own_dist_info':[{'name':x['name'],'version':x['version']} for x in amd['python_distributions'] if not x['license_files']],
            'python_notice_scope':'Only license-like paths inside each own .dist-info are attributed here. Notices elsewhere in package/RECORD may exist; this is not proof that the distribution ships no license. Full attribution/linkage remains unqualified.',
            'unknowns':amd['unknowns']}
    save(args.output, {'schema':'am255/architecture-bom-comparison-v1','results':results,
        'any_amd64_execution':False,'legal_acceptance':False,'profile_acceptance':False,'G1_acceptance':False,
        'scope':'Contents parity/differences and metadata declarations; no amd64 graph/query/ABI execution, no native/emulation equivalence'})
    print(json.dumps({k:{'packages':v.get('installed_packages'),'python':v.get('python'),'modules':v.get('query_modules'),'legal':v.get('memgraph_legal')} for k,v in results.items()},indent=2))

if __name__ == '__main__':
    main()
