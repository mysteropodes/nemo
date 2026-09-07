#!/usr/bin/env python3
"""Verify a snapshot and restore every archived ref in a new empty repository."""
import argparse,datetime,hashlib,json,pathlib,subprocess,tempfile

def run(*args):
    return subprocess.run(args,check=True,text=True,capture_output=True)

def verify(snapshot):
    snapshot=snapshot.resolve()
    for line in (snapshot/'SHA256SUMS').read_text().splitlines():
        expected,name=line.split('  ',1)
        target=snapshot/name
        if target.parent!=snapshot or not target.is_file():
            raise ValueError('Unexpected checksum entry: '+name)
        actual=hashlib.sha256(target.read_bytes()).hexdigest()
        if actual!=expected:
            raise ValueError('SHA-256 mismatch: '+name)
    manifest=json.loads((snapshot/'manifest.json').read_text())
    audit=json.loads((snapshot/'audit.json').read_text())
    bundle=snapshot/manifest['bundle']
    expected={r['archive_ref']:r['sha'] for r in audit['refs'] if r['disposition']=='candidate'}
    with tempfile.TemporaryDirectory(prefix='nemo-archive-restore-') as temp:
        git=pathlib.Path(temp)/'restore.git'
        run('git','init','--bare',str(git))
        verified=run('git','-C',str(git),'bundle','verify',str(bundle))
        # Empty repository + bundle verify proves there are no external prerequisites.
        run('git','-C',str(git),'fetch','--no-tags',str(bundle),'+refs/archive/*:refs/archive/*')
        listing=run('git','-C',str(git),'for-each-ref','--format=%(refname) %(objectname)','refs/archive/').stdout
        actual=dict(line.split() for line in listing.splitlines())
        if actual!=expected:
            raise ValueError('Restored refs do not exactly match the audited tips')
        run('git','-C',str(git),'fsck','--full','--no-reflogs','--no-dangling')
        for ref,sha in expected.items():
            observed=run('git','-C',str(git),'rev-parse',ref+'^{commit}').stdout.strip()
            if observed!=sha:raise ValueError('Not the original commit: '+ref)
    return {'snapshot':manifest['snapshot'],'verified_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'bundle_sha256':manifest['bundle_sha256'],'bundle_bytes':bundle.stat().st_size,
            'restored_refs':len(expected),'unique_tip_shas':len(set(expected.values())),
            'empty_repository_restore':'pass','all_original_tip_shas':'pass','git_fsck_full':'pass',
            'external_git_prerequisites':0}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('snapshot',nargs='?',type=pathlib.Path,default=pathlib.Path(__file__).parent/'snapshots'/'2026-09-07')
    args=parser.parse_args()
    print(json.dumps(verify(args.snapshot),indent=2))
