#!/usr/bin/env python3
"""Actual source indexing, historical Git objects and fresh policy-filtered export smoke."""
import argparse
import json
import pathlib
import sqlite3
import subprocess
import sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from kg_bundle import digest
REPO = pathlib.Path(__file__).resolve().parents[2]



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=pathlib.Path, required=True)
    parser.add_argument('--repo', type=pathlib.Path, default=REPO, help='Git checkout to index; defaults to this KG repository')
    parser.add_argument('--revision', default='HEAD')
    parser.add_argument('--source', default='kg-source')
    parser.add_argument('--origin', default='https://github.com/Mudcrab-Team/mudcrab-knowledge-graph')
    parser.add_argument('--query', default='Store', help='A CodeSymbol in the indexed source')
    parser.add_argument('--binary', type=pathlib.Path, default=REPO / 'target/debug/mudcrab-kg')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve()
    revision = subprocess.run(['git', '-C', str(args.repo), 'rev-parse', '--verify', args.revision + '^{commit}'], check=True, capture_output=True, text=True, timeout=30).stdout.strip()
    root = args.out / 'graph'

    def kg(*argv, store=root):
        result = subprocess.run([str(binary), '--root', str(store), *map(str, argv)], check=True,
                                capture_output=True, text=True, timeout=600)
        return json.loads(result.stdout)

    def save(name, value):
        path = args.out / (name + '.json')
        path.write_text(json.dumps(value, indent=2, ensure_ascii=False))
        return path

    def context(name, snapshots, policy='public'):
        manifest = save(name + '-manifest', {'schema_version': 1, 'name': name,
                                           'snapshot_ids': snapshots, 'policy': policy})
        return kg('context', 'create', '--name', name, '--manifest', manifest)['context_id']

    kg('init')
    source_args = ['ingest', 'source', '--repo', args.repo.resolve(), '--source', args.source, '--revision', revision,
                   '--origin', args.origin]
    source = kg(*source_args)
    repeat = kg(*source_args)
    assert repeat['snapshot_id'] == source['snapshot_id'] and repeat['idempotent']
    ctx = context('source', [source['snapshot_id']])
    found = kg('--context', ctx, 'search', '--query', args.query, '--kind', 'CodeSymbol', '--limit', 2)
    save('q01-source', found)
    node = found['result']['nodes'][0]['node']['id']
    packet = kg('--context', ctx, 'context', 'pack', '--seed', node)
    assert len(json.dumps(packet, separators=(',', ':'), ensure_ascii=False).encode()) <= 65536
    save('q08-source-packet', packet)
    save('source-coverage', kg('--context', ctx, 'coverage'))
    save('source-evidence', kg('--context', ctx, 'evidence', '--id', found['evidence'][0]['id']))
    missing = kg('--context', ctx, 'search', '--query', 'NoSuchOriginalSmokeDeclaration')
    assert missing['result']['status'] == 'no_match_in_indexed_scope'
    save('q10-no-match', missing)

    # Real Git history, with a deleted declaration and an immutable old context.
    history = args.out / 'original-history'
    history.mkdir()
    (history / 'toy.rs').write_text('pub fn kept() {}\npub fn removed() {}\n')
    def git(*argv):
        return subprocess.run(['git', '-C', str(history), *argv], check=True,
                              capture_output=True, text=True, timeout=30).stdout.strip()
    git('init'); git('add', '.')
    def commit(message):
        git('-c', 'user.name=KG Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-m', message)
        return git('rev-parse', 'HEAD')
    rev1 = commit('original source')
    def index(revision):
        return kg('ingest', 'source', '--repo', history, '--source', 'original-history', '--revision', revision,
                  '--origin', 'original:source-history')
    first = index(rev1)
    old = context('history', [first['snapshot_id']])
    (history / 'toy.rs').write_text('pub fn kept() {}\n'); git('add', '.')
    rev2 = commit('remove declaration')
    second = index(rev2)
    new = context('history', [second['snapshot_id']])
    old_result = kg('--context', old, 'search', '--query', 'removed')
    new_result = kg('--context', new, 'search', '--query', 'removed')
    assert old_result['result']['nodes'] and not new_result['result']['nodes'] and old != new
    save('q07-history', {'old': old_result, 'new': new_result, 'old_revision': rev1, 'new_revision': rev2})

    # An original privacy fixture is deliberately restricted. It is not debugger evidence.
    marker = 'RESTRICTED_ORIGINAL_POLICY_SMOKE_MARKER'
    payload = {'original_privacy_fixture': marker}
    observation = {'schema_version': 1, 'session_id': 'original-policy-smoke', 'synthetic': False,
                   'producer': {'name': 'original-privacy-fixture', 'version': '1'}, 'time_domain': 'sequence',
                   'modules': [], 'observations': [{'record_id': '1', 'kind': 'object', 'time': '1',
                   'payload': payload, 'payload_sha256': digest(payload), 'identity': {'kind': 'heap', 'va': '0x1000'}}]}
    restricted = kg('ingest', 'observations', '--file', save('restricted-original-input', observation),
                    '--source', 'restricted-policy-smoke')
    mixed = context('mixed', [source['snapshot_id'], restricted['snapshot_id']], 'research-local')
    public = args.out / 'public-export'
    exported = kg('--context', mixed, 'export', '--policy', 'public', '--out', public)
    public_result = kg('--context', 'public', 'search', '--query', marker, store=public)
    assert not public_result['result']['nodes']
    for path in public.rglob('*'):
        if path.is_file():
            assert marker.encode() not in path.read_bytes()
            assert path.name not in ('graph.sqlite-wal', 'graph.sqlite-shm', 'local-roots.json')
    with sqlite3.connect(public / 'graph.sqlite') as db:
        assert db.execute('SELECT count(*) FROM source_cache').fetchone()[0] == 0
    public_found = kg('--context', 'public', 'search', '--query', args.query, '--kind', 'CodeSymbol', '--limit', 2, store=public)
    public_packet = kg('--context', 'public', 'context', 'pack', '--seed', public_found['result']['nodes'][0]['node']['id'], store=public)
    assert marker not in json.dumps(public_packet)
    save('q09-public-packet', public_packet)
    save('public-export-result', exported)
    report = {'schema_version': 1, 'revision': revision, 'source_id': args.source, 'source': source, 'source_reimport': repeat,
              'context_id': ctx, 'history': {'old_context': old, 'new_context': new, 'deleted_declaration_retained_only_in_old': True},
              'source_packet_bytes': packet['result']['byte_count'], 'public_packet_bytes': public_packet['result']['byte_count'],
              'public_export': 'fresh store; marker absent from every file and FTS; parse cache empty; no sidecars',
              'q10': 'no_match_in_indexed_scope; native/capture connections remain not_indexed'}
    save('result', report)
    print(json.dumps(report))


if __name__ == '__main__':
    main()
