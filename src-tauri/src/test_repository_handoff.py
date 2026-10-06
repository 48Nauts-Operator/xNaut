"""Deterministic handoff tests. No real signals, provider, or home writes."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import repository_handoff as h


class HandoffTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.home = Path(self.temp.name).resolve()
        self.root = self.home / 'agents/runs/author'
        (self.root / '.git').mkdir(parents=True)
        self.expected = dict(run_id='author', project='TEST', ticket='TEST-1', handle='author',
                             workdir='agents/runs/author', artifacts='.xnaut/runs/author',
                             branch='task', source_sha='a' * 40, remote='ssh://fixture/repo.git',
                             head='b' * 40, agent_pid=11, environment='exe-dev')
        self.write('xnaut-transfer.json', self.expected)
        self.write('xnaut-upload.json', dict(state='pushed', head=self.expected['head']))
        (self.root / '.git/xnaut-phase').write_text('running')
        (self.root / '.git/xnaut-supervisor.pid').write_text('10')
        self.supervisor = dict(pid=10, parent=1, session=10, birth='100', cwd=str(self.root))
        self.child = dict(pid=11, parent=10, session=10, birth='101', cwd=str(self.root))
        self.helper = dict(pid=12, parent=11, session=10, birth='102', cwd=str(self.root))
        self.rows = [self.supervisor, self.child, self.helper]
        self.result = dict(run_id='author', source_sha='a' * 40, exit_code=0, uncommitted_source=False)
        self.dirty = ''
        self.signals = []
        self.patches = [patch.object(h, 'processes', lambda: self.rows),
                        patch.object(h, 'process', lambda pid: next((r for r in self.rows if r['pid'] == pid), None)),
                        patch.object(h, 'git', self.git),
                        patch.object(h.os, 'pidfd_open', lambda pid: pid, create=True),
                        patch.object(h.os, 'close', lambda fd: None),
                        patch.object(h.signal, 'pidfd_send_signal', lambda fd, sig: self.signals.append((fd, sig)), create=True)]
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in reversed(self.patches):
            p.stop()
        self.temp.cleanup()

    def write(self, name, value):
        (self.root / '.git' / name).write_text(json.dumps(value))

    def git(self, root, *args):
        if args[0] == 'rev-parse':
            return self.expected['head']
        if args[0] == 'status':
            return self.dirty
        return json.dumps(self.result)

    def call(self):
        return h.handoff(self.expected, self.home)

    def test_legacy_supervisor_handoff_waits_for_every_writer_and_real_finish(self):
        result_before = dict(self.result)
        first = self.call()
        self.assertEqual(first['state'], 'pending')
        self.assertEqual(self.signals, [(11, h.signal.SIGTERM)])
        self.assertTrue((self.root / '.git/xnaut-handoff.json').exists())
        self.assertEqual((self.root / '.git/xnaut-phase').read_text(), 'running')
        # Even a finished phase is insufficient while a background child lives.
        (self.root / '.git/xnaut-phase').write_text('finished')
        self.rows = [self.helper]
        self.assertEqual(self.call()['state'], 'pending')
        self.assertEqual(len(self.signals), 1)
        self.rows = []
        self.assertEqual(self.call()['state'], 'finished')
        self.assertEqual(self.call()['state'], 'finished')
        self.assertEqual(self.result, result_before, 'accepted exit code is never rewritten')
        # A historical finished receipt never proves today's writer absence.
        self.rows = [dict(pid=99, parent=1, session=99, birth='999', cwd=str(self.root))]
        self.assertEqual(self.call()['state'], 'pending')
        self.assertEqual(len(self.signals), 1)

    def test_exact_environment_paths_preserve_gitvm_and_refuse_relocation(self):
        gitvm = dict(self.expected, environment='gitvm', workdir='/workspace/.xnaut-runs/author')
        self.assertEqual(h.worker_root(gitvm, self.home), Path('/workspace/.xnaut-runs/author'))
        self.assertEqual(h.worker_root(self.expected, self.home), self.root)
        for bad in [dict(gitvm, workdir=str(self.root)), dict(self.expected, workdir='/other/author')]:
            with self.assertRaises(ValueError):
                h.worker_root(bad, self.home)

    def test_busy_publisher_and_exit_race_wait_without_false_failure(self):
        with (self.root / '.git/xnaut-publish.lock').open('a') as publisher:
            h.fcntl.flock(publisher, h.fcntl.LOCK_EX)
            self.assertEqual(self.call()['state'], 'pending')
            self.assertEqual(self.signals, [])
        with patch.object(h.os, 'pidfd_open', side_effect=ProcessLookupError, create=True):
            self.assertEqual(self.call()['state'], 'pending')
        self.assertEqual(self.signals, [])
        self.rows = []
        (self.root / '.git/xnaut-phase').write_text('finished')
        self.assertEqual(self.call()['state'], 'finished')

    def test_wrong_identity_dirty_or_unpublished_work_never_receives_signal(self):
        for key in ['run_id', 'project', 'ticket', 'handle', 'branch', 'remote']:
            bad = dict(self.expected, **{key: 'wrong'})
            self.write('xnaut-transfer.json', bad)
            with self.assertRaises(ValueError):
                self.call()
        self.write('xnaut-transfer.json', self.expected)
        self.dirty = ' M app.py'
        with self.assertRaises(ValueError):
            self.call()
        self.dirty = ''
        self.write('xnaut-upload.json', dict(state='pending', head=self.expected['head']))
        with self.assertRaises(ValueError):
            self.call()
        self.assertEqual(self.signals, [])
        self.assertFalse((self.root / '.git/xnaut-handoff.json').exists())

    def test_pid_reuse_and_foreign_workspace_are_not_authority(self):
        self.call()
        reused = dict(self.child, birth='999')
        self.rows = [self.supervisor, reused]
        self.assertEqual(self.call()['state'], 'pending')
        self.assertEqual(len(self.signals), 1, 'a reused PID must never be signaled')
        self.rows = [self.supervisor, dict(self.child, cwd='/another-task')]
        with self.assertRaises(ValueError):
            self.call()
        self.assertEqual(len(self.signals), 1)

    def test_publisher_and_new_background_work_keep_the_handoff_pending(self):
        self.call()
        publisher = dict(pid=13, parent=10, session=10, birth='103', cwd=str(self.root))
        self.rows = [self.supervisor, publisher]
        (self.root / '.git/xnaut-phase').write_text('uploading')
        self.assertEqual(self.call()['state'], 'pending')
        (self.root / '.git/xnaut-phase').write_text('finished')
        self.rows = [dict(publisher, session=13, parent=1)]
        self.assertEqual(self.call()['state'], 'pending')
        self.assertEqual(self.signals, [(11, h.signal.SIGTERM)])


if __name__ == '__main__':
    unittest.main()
