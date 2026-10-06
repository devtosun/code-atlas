#!/usr/bin/env python3
"""Adversarial CR-001..013 checks against a supplied, freshly built native binary.

All repositories, homes, databases and configuration writes are disposable.
Run alongside the Rust migration, extraction and cancellation tests.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import sqlite3
import subprocess
import tempfile
import time

from phase14_measure import McpClient, modern_meta


class Suite:
    def __init__(self, binary: Path, base: Path):
        self.binary, self.base = binary, base
        self.checks = []

    def layout(self, name, files):
        root, home = self.base / name / 'repo', self.base / name / 'home'
        root.mkdir(parents=True)
        home.mkdir()
        for name, content in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content.encode() if isinstance(content, str) else content)
        env = {**os.environ, 'HOME': str(home), 'CODEX_HOME': str(home / '.codex')}
        return root, home, env

    def cli(self, root, env, *args, good=True):
        result = subprocess.run([str(self.binary), *args, '--root', str(root), '--json'],
                                env=env, capture_output=True, text=True, timeout=30)
        assert (result.returncode == 0) == good, (args, result.stderr, result.stdout)
        return json.loads(result.stdout) if good else result

    def db(self, root, env):
        path = self.cli(root, env, 'doctor')['configuration']['database_path']
        return sqlite3.connect(path)

    def calls(self, db):
        return db.execute("""SELECT f.relative_path, c.spelling, e.resolution, tf.relative_path, s.spelling
            FROM resolved_edges e JOIN file_versions v ON v.id=e.source_file_version_id
            JOIN files f ON f.id=v.file_id JOIN call_sites c
            ON c.file_version_id=v.id AND c.observation_id=e.source_observation_id
            LEFT JOIN file_versions tv ON tv.id=e.target_file_version_id
            LEFT JOIN files tf ON tf.id=tv.file_id LEFT JOIN symbols s
            ON s.file_version_id=tv.id AND s.observation_id=e.target_symbol_id
            WHERE e.relationship='calls' AND e.generation_id=(SELECT active_generation_id FROM meta)""").fetchall()

    def record(self, name, **evidence):
        self.checks.append({'check': name, 'passed': True, **evidence})
        print(name + ': PASS', flush=True)

    def semantic(self):
        root, _, env = self.layout('scope', {'a.rs': 'fn main(){target();}'})
        self.cli(root, env, 'index')
        self.record('CR-001 EOF-equal scopes terminate')
        root, _, env = self.layout('invalid', {'a.rs': 'pub fn healthy() {}\n'})
        self.cli(root, env, 'index')
        with self.db(root, env) as db:
            active = db.execute('SELECT active_generation_id FROM meta').fetchone()[0]
        for bytes_ in [b'\xff', b'\x00', b'x' * (2 * 1024 * 1024 + 1)]:
            (root / 'a.rs').write_bytes(bytes_)
            for mode in [[], ['--full']]:
                self.cli(root, env, 'index', *mode, good=False)
                with self.db(root, env) as db:
                    assert db.execute('SELECT active_generation_id FROM meta').fetchone()[0] == active
        (root / 'a.rs').unlink()
        self.cli(root, env, 'index')
        with self.db(root, env) as db:
            assert not db.execute('SELECT * FROM generation_files WHERE generation_id=(SELECT active_generation_id FROM meta)').fetchall()
        self.record('CR-002 invalid/binary/oversized preserve active; real delete works')

        members = {
            'rs': 'struct S {value:i32} fn read(obj:S){let value=9;let x=obj.value;let y=value;}',
            'go': 'package p\ntype S struct {Value int};func read(obj S){Value:=9;x:=obj.Value;_=x;_=Value}',
            'cs': 'class S {int value;void Read(S obj){int value=9;int x=obj.value;int y=value;}}',
            'java': 'class S {int value;void read(S obj){int value=9;int x=obj.value;int y=value;}}',
            'dart': 'class S {int value=1;void read(S obj){var value=9;var x=obj.value;var y=value;}}',
        }
        for ext, source in members.items():
            root, _, env = self.layout('member-' + ext, {'a.' + ext: source})
            self.cli(root, env, 'index')
            with self.db(root, env) as db:
                rows = db.execute("""SELECT r.receiver, e.resolution, s.kind FROM resolved_edges e
                    JOIN "references" r ON r.file_version_id=e.source_file_version_id AND r.observation_id=e.source_observation_id
                    LEFT JOIN symbols s ON s.file_version_id=e.target_file_version_id AND s.observation_id=e.target_symbol_id
                    WHERE e.generation_id=(SELECT active_generation_id FROM meta) AND r.spelling IN ('value','Value')""").fetchall()
                selected = [row for row in rows if row[0] == 'obj']
                assert selected and all(row[1] in ('candidate', 'unresolved') for row in selected), (ext, rows)
                assert all(row[2] not in ('local', 'variable') for row in selected), (ext, rows)
                assert any(row[0] is None and row[1] == 'lexically_resolved' for row in rows), (ext, rows)
        self.record('CR-003 five member selectors stay uncertain; bare names resolve')

        root, _, env = self.layout('go-package', {
            'a/provider.go': 'package shared\nfunc OnlyA() {}\n',
            'a/use.go': 'package shared\nfunc RunA(){ OnlyA() }\n',
            'b/use.go': 'package shared\nfunc RunB(){ OnlyA() }\n',
        })
        self.cli(root, env, 'index')
        with self.db(root, env) as db:
            rows = self.calls(db)
            assert any(row[:3] == ('a/use.go', 'OnlyA', 'lexically_resolved') for row in rows), rows
            assert any(row[:3] == ('b/use.go', 'OnlyA', 'unresolved') for row in rows), rows
        self.record('CR-004 Go same-directory positive, other-directory negative')

        root, _, env = self.layout('rust-import', {
            'src/lib.rs': 'mod other;fn inside(){use crate::other::target as alias;alias();} fn outside(){alias();}\n',
            'src/other.rs': 'pub fn target() {}\n',
        })
        self.cli(root, env, 'index')
        with self.db(root, env) as db:
            rows = [row for row in self.calls(db) if row[1] == 'alias']
            assert len(rows) == 2 and sorted(row[2] for row in rows) == ['lexically_resolved', 'unresolved'], rows
        self.record('CR-005 local use alias does not escape its scope')

        root, _, env = self.layout('rust-alias-shadow', {
            'src/lib.rs': 'mod outer;mod inner;use crate::outer::target as alias;fn run(){alias();{use crate::inner::target as alias;alias();}alias();}\n',
            'src/outer.rs': 'pub fn target() {}\n',
            'src/inner.rs': 'pub fn target() {}\n',
        })
        self.cli(root, env, 'index')
        with self.db(root, env) as db:
            rows = [row for row in self.calls(db) if row[1] == 'alias']
            assert len(rows) == 3 and all(row[2] == 'lexically_resolved' for row in rows), rows
            assert sorted(row[3] for row in rows) == ['src/inner.rs', 'src/outer.rs', 'src/outer.rs'], rows
        self.record('CR-005 nearest nested alias shadows outer import only within block')

        root, _, env = self.layout('dart-combinators', {
            'a.dart': 'void allowed(){} void hidden(){}\n',
            'exports.dart': "export 'a.dart' show allowed hide hidden;\n",
            'b.dart': "import 'exports.dart' show allowed;void run(){allowed();hidden();}\n",
            'c.dart': "import 'a.dart' as p hide hidden;void run(){p.allowed();p.hidden();}\n",
        })
        self.cli(root, env, 'index')
        with self.db(root, env) as db:
            rows = self.calls(db)
            assert all(row[2] == 'unresolved' for row in rows if row[1].endswith('hidden')), rows
            assert len([row for row in rows if row[1].endswith('allowed') and row[3] == 'a.dart']) == 2, rows
        self.record('CR-009 import/export show/hide and prefix rules')

        for ext in ['js', 'jsx', 'ts', 'tsx']:
            root, _, env = self.layout('exports-' + ext, {
                'a.' + ext: 'export const arrow=()=>1;export const expression=function(){return 1};const local=()=>2;export {local as renamed};export default local;\n',
                'b.' + ext: "import fallback,{arrow,expression,renamed} from './a';export function run(){arrow();expression();renamed();fallback();}\n",
            })
            self.cli(root, env, 'index')
            with self.db(root, env) as db:
                rows = self.calls(db)
                assert all(any(row[1] == name and row[2] == 'lexically_resolved' and row[3] == 'a.' + ext for row in rows)
                           for name in ['arrow', 'expression', 'renamed', 'fallback']), rows
        self.record('CR-010 four ECMAScript dialects: arrows, expressions, aliases, defaults')

    def storage(self):
        root, _, env = self.layout('retention', {'a.rs': 'pub fn original() {}\n'})
        self.cli(root, env, 'index')
        with self.db(root, env) as db:
            db.execute("INSERT INTO memories(id,body,created_at) VALUES ('sentinel','keep me',1)")
        for n in range(30):
            if n % 2:
                (root / 'a.rs').write_text(f'pub fn revision_{n}() {{}}\n')
            self.cli(root, env, 'index')
        with self.db(root, env) as db:
            assert db.execute('SELECT count(*) FROM generations').fetchone()[0] == 2
            assert db.execute('SELECT count(*) FROM file_versions').fetchone()[0] <= 2
            assert db.execute("SELECT body FROM memories WHERE id='sentinel'").fetchone()[0] == 'keep me'
            assert not db.execute('PRAGMA foreign_key_check').fetchall()
        self.record('CR-008 30 changed/no-change generations bounded; notes survive')

        root, _, env = self.layout('queue-failure', {'a.rs': 'pub fn healthy() {}\n'})
        self.cli(root, env, 'index')
        with self.db(root, env) as db:
            active = db.execute('SELECT active_generation_id FROM meta').fetchone()[0]
            db.execute("CREATE TRIGGER fail_stage BEFORE INSERT ON symbols BEGIN SELECT RAISE(FAIL,'injected persistence failure'); END")
        for n in range(128):
            (root / f'item{n}.rs').write_text(f'pub fn item{n}() {{}}\n')
        self.cli(root, env, 'index', '--full', good=False)
        with self.db(root, env) as db:
            assert db.execute('SELECT active_generation_id FROM meta').fetchone()[0] == active
            db.execute('DROP TRIGGER fail_stage')
        outcome = self.cli(root, env, 'index', '--full')
        self.record('CR-007 >queue corpus survives persistence failure without deadlock', outcome=outcome)

    def wire(self):
        for modern in [False, True]:
            root, home, _ = self.layout('wire-' + str(modern), {})
            client = McpClient(self.binary, root, home, modern)
            client.initialize()
            response = client.request('prompts/get', {'name': 'plan_change', 'arguments': {'objective': 'x' * 70_000}})
            assert 'error' in response, response
            response = client.request('prompts/get', {'name': 'investigate_failure', 'arguments': {'failure': 'ok', 'scope': 'x' * 4_097}})
            assert 'error' in response, response
            assert 'result' in client.request('tools/list', {})
            params = {'name': 'repository_status', 'arguments': {}}
            if modern:
                params['_meta'] = modern_meta()
            for request_id in ['é' * 63, '\x01' * 21]:
                client._send({'jsonrpc': '2.0', 'id': request_id, 'method': 'tools/call', 'params': params})
                assert client._receive()['id'] == request_id
            response = client.request('prompts/get', {'name': 'plan_change', 'arguments': {'objective': '\x01' * 8_192, 'scope': '\x01' * 4_096}})
            assert 'error' in response
            client.stop()
            client = McpClient(self.binary, root, home, modern)
            client.initialize()
            params = {'name': 'repository_status', 'arguments': {}}
            if modern:
                params['_meta'] = modern_meta()
            client._send({'jsonrpc': '2.0', 'id': 'x' * 70_000, 'method': 'tools/call', 'params': params})
            ready, _, _ = select.select([client.process.stdout], [], [], 10)
            assert ready and client.process.stdout.readline() == '', 'oversized ID must terminate before echo'
            client.process.wait(timeout=10)
            assert client.process.returncode == 0
            # An SDK-generated routing error may contain a hostile long name.
            # It must be bounded or terminate cleanly, never emit an oversized line.
            client = McpClient(self.binary, root, home, modern)
            client.initialize()
            params = {'name': 'x' * 70_000, 'arguments': {}}
            if modern:
                params['_meta'] = modern_meta()
            client._send({'jsonrpc': '2.0', 'id': 99, 'method': 'tools/call', 'params': params})
            ready, _, _ = select.select([client.process.stdout], [], [], 10)
            assert ready, 'SDK over-budget response must not leave a waiting client'
            line = client.process.stdout.readline()
            assert len(line.encode()) <= 65_536
            if line:
                assert 'error' in json.loads(line)
                client.stop()
            else:
                client.process.wait(timeout=10)
                assert client.process.returncode == 0
            client = McpClient(self.binary, root, home, modern)
            client.initialize()
            try:
                client._send({'jsonrpc': '2.0', 'id': 100, 'method': 'tools/call',
                              'params': {'name': 'x' * 1_048_576, 'arguments': {}}})
            except BrokenPipeError:
                pass  # SDK decoder may close while the oversized input is being sent.
            ready, _, _ = select.select([client.process.stdout], [], [], 10)
            assert ready and client.process.stdout.readline() == '', 'oversized ingress must close without echo'
            client.process.wait(timeout=10)
            assert client.process.returncode == 0
        self.record('CR-012 complete wire frame bounds in legacy and modern protocols')

        root, _, env = self.layout('config-canary', {})
        config = root.parent / 'config.toml'
        canary = 'PRIVATE_CANARY_7ba9'
        content = f'api_key = "{canary}" malformed\n'.encode()
        config.write_bytes(content)
        for action in [['--dry-run'], ['--apply'], ['--remove', '--dry-run'], ['--remove', '--apply']]:
            response = subprocess.run([str(self.binary), 'integrate', 'codex', '--root', str(root), '--config', str(config), *action],
                                      capture_output=True, env=env, timeout=10)
            assert response.returncode != 0 and canary.encode() not in response.stdout + response.stderr
            assert b'malformed Codex config' in response.stderr
            assert config.read_bytes() == content
        self.record('CR-013 malformed private config never echoed, all modes unchanged')

    def pagination(self):
        root, home, env = self.layout('pagination', {
            f'a{n:03}.rs': '\n'.join(f'pub fn Same(){{}} // {m}' for m in range(100)) + '\n'
            for n in range(110)
        })
        self.cli(root, env, 'index')
        client = McpClient(self.binary, root, home, True)
        client.initialize()
        cursor, seen, pages = None, set(), 0
        while True:
            args = {'query': 'Same', 'limit': 40}
            if cursor:
                args['cursor'] = cursor
            response = client.tool('search_symbols', args)
            envelope = response['result']['structuredContent']
            data = envelope['data']
            items = data['results']
            for item in items:
                key = (item['symbol']['relative_path'], item['symbol']['id'])
                assert key not in seen, key
                seen.add(key)
            pages += 1
            cursor = envelope.get('next_cursor') or data.get('next_cursor')
            if not cursor:
                assert not envelope['truncated'], envelope
                break
            assert pages < 1000 and items
        client.stop()
        assert len(seen) == 11_000, len(seen)
        self.record('CR-011 keyset pagination passes former 10k ceiling', returned=len(seen), pages=pages)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--only', action='append', choices=['semantic', 'storage', 'wire', 'pagination'],
                        help='Repeat selected boundaries without rerunning the large pagination corpus')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    started = time.perf_counter()
    with tempfile.TemporaryDirectory(prefix='codeatlas-review-fixes-') as directory:
        suite = Suite(binary, Path(directory))
        for group in ['semantic', 'storage', 'wire', 'pagination']:
            if not args.only or group in args.only:
                getattr(suite, group)()
        report = {'schema_version': 1, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                  'elapsed_seconds': time.perf_counter() - started, 'checks': suite.checks}
    output = json.dumps(report, indent=2) + '\n'
    if args.output:
        args.output.write_text(output)
    print(output)


if __name__ == '__main__':
    main()
