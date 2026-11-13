#!/usr/bin/env python3
"""Read-only Cargo/source inspection. Writes only this new evidence directory.

No counterfactual graph is called a compiled artifact. No upstream edits.
"""
import gzip
import hashlib
import json
import pathlib
import re
import subprocess
import tomllib
import tarfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BASE = '6e64e1d6ac359ea223baf6754d219ca2b2148a82'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)


def snapshot(phase):
    assert phase in {'before', 'after'}
    result = {}
    for profile in ['full-daemon', 'tui', 'full-daemon,iroh-transport']:
        cmd = ['cargo', 'tree', '--locked', '--offline', '-p', 'medousa',
               '--no-default-features', '--features', profile, '-e', 'normal,build',
               '--prefix', 'none', '--format', '{p} FEATURES=[{f}]']
        text = subprocess.check_output(cmd, cwd=ROOT, text=True)
        name = f'{phase}-{profile.replace(",", "-")}'
        (HERE / (name + '.txt.gz')).write_bytes(gzip.compress(text.encode(), mtime=0))
        packages = sorted(set(re.findall(r'^([^ ]+ v[^ ]+) ', text, re.M)))
        features = {}
        for line in text.splitlines():
            m = re.match(r'^(\S+ v\S+) .*FEATURES=\[([^\]]*)\]', line)
            if m:
                features[m[1]] = sorted(filter(None, m[2].split(',')))
        result[profile] = {'command': cmd, 'graph_sha256': sha(text.encode()),
                           'packages': packages, 'features': features}
        modules = {'data', 'pdf', 'image', 'plot', 'media'}
        for package, required in {
            'grapheme-sdk': modules | {'host', 'stage-b', 'wasix-runtime'},
            'grapheme-stdlib': modules | {'host', 'transforms', 'http', 'web', 'net', 'email', 'sql', 'surreal'},
            'grapheme-runtime': modules | {'stage-b', 'wasix-runtime'},
            'grapheme-compiler': modules | {'stage-b', 'full'},
            'grapheme-signatures': modules | {'full'},
        }.items():
            assert required <= set(features[package + ' v0.7.1']), (profile, package)
        assert 'grapheme-aot-container v0.7.1' in features
        assert any(p.startswith('wasmer-wasix v') for p in packages)
        print(phase, profile, len(packages), 'package-version pairs; retained capability closure PASS')
    (HERE / (phase + '.json')).write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    if phase == 'after':
        assert result == json.loads((HERE / 'before.json').read_text())
        print('PASS exact before/after graph, package and resolved-feature equality; ZERO reduction')
    return result


def sources():
    # git archive reads baseline blobs in one bounded stream, without checkout.
    # Record working bytes AND git blobs; declared CRLF checkouts are not edits.
    baseline = {}
    eol = git('ls-files', '--eol').decode()
    (HERE / 'checkout-eol.txt').write_text(eol)
    crlf = {line.split('\t', 1)[1] for line in eol.splitlines()
            if 'w/crlf' in line and 'eol=crlf' in line}
    proc = subprocess.Popen(['git', 'archive', '--format=tar', BASE], cwd=ROOT, stdout=subprocess.PIPE)
    with tarfile.open(fileobj=proc.stdout, mode='r|') as archive:
        for member in archive:
            if not member.isfile():
                continue
            p = member.name
            blob = archive.extractfile(member).read()
            working = (ROOT / p).read_bytes()
            # git archive applies committed eol attributes, just like checkout.
            assert working == blob, p
            baseline[p] = {'working_sha256': sha(working), 'baseline_archive_sha256': sha(blob),
                           'declared_crlf': p in crlf}
    assert proc.wait() == 0
    (HERE / 'baseline-files.json').write_text(json.dumps(baseline, indent=2, sort_keys=True) + '\n')
    registries = list((pathlib.Path.home() / '.cargo/registry/src').glob('*'))
    manifest_data = {}
    source_data = {}
    excerpts = []
    names = ['grapheme-sdk-0.7.1', 'grapheme-stdlib-0.7.1', 'grapheme-signatures-0.7.1',
             'grapheme-compiler-0.7.1', 'grapheme-runtime-0.7.1', 'grapheme-lsp-0.7.1',
             'grapheme-wasm-0.7.1', 'grapheme-aot-container-0.7.1', 'stasis-rs-0.13.0',
             'polars-0.46.0', 'polars-core-0.46.0', 'comfy-table-7.2.2']
    for name in names:
        roots = [r / name for r in registries if (r / name).exists()]
        assert len(roots) == 1, (name, roots)
        r = roots[0]
        raw = (r / 'Cargo.toml').read_bytes()
        d = tomllib.loads(raw.decode())
        manifest_data[name] = {'sha256': sha(raw), 'features': d.get('features', {}),
                               'dependencies': d.get('dependencies', {}),
                               'target': d.get('target', {})}
        (HERE / (name + '-Cargo.toml')).write_bytes(raw)
        for p in (r / 'src').rglob('*.rs'):
            if not name.startswith(('grapheme-', 'stasis-')):
                continue
            rel = name + '/' + str(p.relative_to(r))
            source_data[rel] = sha(p.read_bytes())
            # Compact line-addressable evidence of cfg/dispatch/disabled paths.
            if name in {'grapheme-sdk-0.7.1', 'grapheme-stdlib-0.7.1', 'grapheme-runtime-0.7.1'}:
                for i, line in enumerate(p.read_text().splitlines(), 1):
                    if any(t in line for t in ['#[cfg(feature', 'grapheme_stdlib::registry::dispatch',
                                               'runtime built without wasix-runtime',
                                               'stage_b execution is disabled', 'feature is not enabled']):
                        excerpts.append(f'{rel}:{i}: {line}')
    (HERE / 'upstream-manifests.json').write_text(json.dumps(manifest_data, indent=2, sort_keys=True) + '\n')
    (HERE / 'upstream-source-sha256.json').write_text(json.dumps(source_data, indent=2, sort_keys=True) + '\n')
    locked = {p['name'] + '-' + p['version']: p for p in tomllib.loads((ROOT / 'Cargo.lock').read_text())['package']}
    proof = {}
    for name, manifest in manifest_data.items():
        caches = list((pathlib.Path.home() / '.cargo/registry/cache').glob('*/' + name + '.crate'))
        assert len(caches) == 1, name
        digest = sha(caches[0].read_bytes())
        assert digest == locked[name]['checksum'], name
        checked = {'Cargo.toml': manifest['sha256']}
        checked.update({p[len(name)+1:]: v for p, v in source_data.items() if p.startswith(name + '/')})
        with tarfile.open(caches[0]) as archive:
            for p, value in checked.items():
                assert sha(archive.extractfile(name + '/' + p).read()) == value, (name, p)
        proof[name] = {'locked_crate_sha256': digest, 'checked_files': checked}
    (HERE / 'upstream-registry-checksums.json').write_text(json.dumps(proof, indent=2, sort_keys=True) + '\n')
    (HERE / 'compiled-source-gates.txt').write_text('\n'.join(excerpts) + '\n')
    sdk = manifest_data['grapheme-sdk-0.7.1']['features']
    std = manifest_data['grapheme-stdlib-0.7.1']
    lsp = manifest_data['grapheme-lsp-0.7.1']
    stasis = manifest_data['stasis-rs-0.13.0']['features']
    assert 'grapheme-sdk/host' in stasis['grapheme-host']
    assert 'grapheme-host' in stasis['dashboard']
    assert std['dependencies']['polars']['features'] == ['csv', 'fmt']
    assert set(std['features']['host']) == {'transforms', 'http', 'web', 'net', 'email', 'sql', 'surreal'}
    assert 'full' in lsp['dependencies']['grapheme-compiler']['features']
    assert 'full' in lsp['dependencies']['grapheme-signatures']['features']
    assert 'stage-b' in sdk['wasix-runtime']
    print('PASS exact upstream additive-feature witnesses and source gates; baseline files', len(baseline))


if __name__ == '__main__':
    import sys
    if sys.argv[1] == 'sources':
        sources()
    else:
        snapshot(sys.argv[1])
