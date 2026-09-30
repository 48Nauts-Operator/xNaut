import concurrent.futures
import contextlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import shlex
import socketserver
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[2] / 'src-tauri/src'
spec = importlib.util.spec_from_file_location('worker_access', SOURCE / 'worker_access.py')
access = importlib.util.module_from_spec(spec)
spec.loader.exec_module(access)


class WorkerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='xnaut-worker-')
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_fresh_workers_projects_and_repeated_tasks_have_isolated_stable_keys(self):
        remote = 'ssh://git@forge.example:2222/team/one.git'
        with patch.object(access.Path, 'home', return_value=self.root / 'worker-a'):
            with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
                keys = list(pool.map(lambda _: access.identity(remote)[1], range(6)))
            self.assertEqual(len(set(keys)), 1)
            self.assertEqual(keys[0], access.identity(remote.removesuffix('.git'))[1])
            other_project = access.identity(remote.replace('one.git', 'two.git'))[1]
            self.assertNotEqual(keys[0], other_project)
        with patch.object(access.Path, 'home', return_value=self.root / 'worker-b'):
            self.assertNotEqual(keys[0], access.identity(remote)[1])
        files = list(self.root.rglob('id_ed25519'))
        self.assertEqual(len(files), 3)
        self.assertTrue(all(p.stat().st_mode & 0o777 == 0o600 for p in files))
        self.assertTrue(all(p.parent.stat().st_mode & 0o777 == 0o700 for p in files))

    def test_network_ready_requires_no_enrollment_or_credential(self):
        with patch.object(access, 'reachable', return_value=True), patch.object(access, 'run') as run:
            access.network('forge.example', 22, {})
            run.assert_not_called()

    def test_network_enrollment_uses_stdin_supplied_key_in_temporary_private_file(self):
        commands, secret_files = [], []
        def fake_run(command, **kwargs):
            commands.append(command)
            if command[:3] == ['tailscale', 'status', '--json']:
                return subprocess.CompletedProcess(command, 0, '{"BackendState":"NeedsLogin"}', '')
            auth = next((s for s in command if s.startswith('--auth-key=file:')), None)
            if auth:
                path = Path(auth.split('file:', 1)[1]); secret_files.append(path)
                self.assertEqual(path.read_text(), 'fixture-enrollment-secret')
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            return subprocess.CompletedProcess(command, 0, '', '')
        with patch.object(access, 'reachable', side_effect=[False, True]), patch.object(access.shutil, 'which', return_value='/usr/bin/tailscale'), patch.object(access, 'run', side_effect=fake_run):
            access.network('private.example', 2222, {'auth_key':'fixture-enrollment-secret','tags':'tag:workers'})
        self.assertEqual(len(secret_files), 1)
        self.assertFalse(secret_files[0].exists())
        self.assertNotIn('fixture-enrollment-secret', repr(commands))
        self.assertTrue(any('--advertise-tags=tag:workers' in c for c in commands))

    def test_missing_network_setup_and_existing_route_failure_are_distinct(self):
        with patch.object(access, 'reachable', return_value=False):
            with self.assertRaisesRegex(access.SetupError, 'network_setup_required'):
                access.network('private.example', 22, {})
            with patch.object(access.shutil, 'which', return_value='/usr/bin/tailscale'), patch.object(access, 'run', return_value=subprocess.CompletedProcess([], 0, '{"BackendState":"Running"}', '')) as run:
                with self.assertRaisesRegex(access.SetupError, 'network_route_unavailable'):
                    access.network('private.example', 22, {'auth_key':'secret'})
                self.assertEqual(run.call_count, 1)

    def test_tools_install_and_recheck_with_a_provider_login_shell_exec_wrapper(self):
        binaries = self.root / 'bin'; binaries.mkdir()
        installed = self.root / 'installed'
        calls = self.root / 'package-calls'
        scripts = {
            'git': f'if [ "$1" = lfs ]; then test -f {shlex.quote(str(installed))}; fi',
            'apt-get': f'echo install >> {shlex.quote(str(calls))}; touch {shlex.quote(str(installed))}',
            'sudo': 'shift; exec "$@"',
            'python3': 'exit 0', 'tmux': 'exit 0', 'ssh-keygen': 'exit 0', 'ssh': 'exit 0', 'curl': 'exit 0',
            'flock': f'exec {shlex.quote(sys.executable)} -c "import os; os.fstat(9)"',
        }
        for name, body in scripts.items():
            path = binaries / name; path.write_text('#!/bin/sh\n' + body + '\n'); path.chmod(0o755)
        env = {**os.environ, 'PATH':str(binaries) + ':/usr/bin:/bin'}
        # Actual exe.dev login-shell customization. A bare `exec 9>...` loses
        # the lock fd on return from this function, before flock is invoked.
        script = 'exec() { if [ $# -eq 0 ]; then builtin exec; else builtin exec env "$@"; fi; };\n' + (SOURCE/'worker_tools.sh').read_text()
        for _ in range(2):
            result = subprocess.run(['/bin/bash', '-c', script], env=env, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('XNAUT_BOOTSTRAP_TOOLS_READY', result.stdout)
        self.assertEqual(calls.read_text().splitlines(), ['install', 'install'])

    def test_repository_and_lfs_checks_use_real_git_without_pushing_a_branch(self):
        remote = self.root / 'remote.git'
        seed = self.root / 'seed'
        env = {**os.environ, 'GIT_CONFIG_GLOBAL':os.devnull, 'GIT_CONFIG_NOSYSTEM':'1'}
        def git(*args):
            return subprocess.run(['git', *args], env=env, check=True, capture_output=True, text=True).stdout.strip()
        git('init','--bare',str(remote)); git('init','-b','main',str(seed))
        git('-C',str(seed),'config','user.name','Fixture'); git('-C',str(seed),'config','user.email','fixture@localhost')
        (seed/'source.txt').write_text('source')
        git('-C',str(seed),'add','.'); git('-C',str(seed),'commit','-m','Initial')
        git('-C',str(seed),'push',str(remote),'main'); git('--git-dir',str(remote),'symbolic-ref','HEAD','refs/heads/main')
        before = git('--git-dir',str(remote),'show-ref')
        batches = []; code = [200]
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args): pass
            def do_POST(self):
                batches.append((self.headers.get('Authorization'), json.loads(self.rfile.read(int(self.headers['Content-Length'])))))
                self.send_response(code[0]); self.end_headers(); self.wfile.write(b'{"objects":[]}')
        server = socketserver.TCPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        ssh = self.root/'ssh-fixture'
        denied = self.root/'deny-write'
        ssh.write_text('#!/usr/bin/env python3\n' + f'''import json, os, pathlib, sys
command = sys.argv[-1]
if '-G' in sys.argv: sys.exit(0)
if command.startswith('git-lfs-authenticate'):
 print(json.dumps({{'href':'http://127.0.0.1:{server.server_address[1]}/lfs','header':{{'Authorization':'fixture-lfs'}}}}))
elif command.startswith('git-upload-pack'):
 os.execvp('git-upload-pack',['git-upload-pack',{str(remote)!r}])
elif command.startswith('git-receive-pack'):
 if pathlib.Path({str(denied)!r}).exists(): sys.exit(1)
 os.execvp('git-receive-pack',['git-receive-pack',{str(remote)!r}])
else: sys.exit(1)
'''); ssh.chmod(0o755)
        url = 'ssh://git@forge.example/team/repo.git'
        try:
            with patch.dict(os.environ, env):
                access.verify(url, 'xnaut/runs/first', [str(ssh)], access.endpoint(url))
                access.verify(url, 'xnaut/runs/second', [str(ssh)], access.endpoint(url))
                self.assertEqual(git('--git-dir',str(remote),'show-ref'), before)
                self.assertEqual(len(batches), 2)
                self.assertEqual(batches[0], ('fixture-lfs', {'operation':'upload','transfers':['basic'],'objects':[]}))
                code[0] = 401
                with self.assertRaisesRegex(access.SetupError, 'lfs_upload_unavailable'):
                    access.verify(url, 'xnaut/runs/third', [str(ssh)], access.endpoint(url))
                denied.touch()
                with self.assertRaisesRegex(access.SetupError, 'repository_write_denied'):
                    access.verify(url, 'xnaut/runs/fourth', [str(ssh)], access.endpoint(url))
                self.assertEqual(git('--git-dir',str(remote),'show-ref'), before)
        finally:
            server.shutdown(); server.server_close(); thread.join()


if __name__ == '__main__':
    unittest.main()
