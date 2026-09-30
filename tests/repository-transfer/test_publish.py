import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

PUBLISHER = Path(__file__).resolve().parents[2] / 'src-tauri/src/repository_publish.py'

class PublishTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='xnaut-transfer-')
        self.root = Path(self.tmp.name)
        self.remote = self.root / 'remote.git'
        self.repo = self.root / 'worker'
        tools = self.root / 'bin'; tools.mkdir()
        tmux = tools / 'tmux'
        tmux.write_text('#!/usr/bin/env python3\nimport json, pathlib, sys\npathlib.Path(".git/test-tmux-launch.json").write_text(json.dumps(sys.argv[1:]))\n')
        tmux.chmod(0o755)
        self.cmd('git', 'init', '--bare', str(self.remote), cwd=self.root)
        self.cmd('git', 'init', '-b', 'main', str(self.repo), cwd=self.root)
        self.cmd('git', 'config', 'user.name', 'Test')
        self.cmd('git', 'config', 'user.email', 'test@localhost')
        self.cmd('git', 'lfs', 'install', '--local')
        (self.repo/'source.txt').write_text('original\n')
        (self.repo/'.gitignore').write_text('.xnaut/*\n')
        self.cmd('git', 'add', 'source.txt', '.gitignore')
        self.cmd('git', 'commit', '-m', 'initial')
        self.sha = self.cmd('git', 'rev-parse', 'HEAD').stdout.strip()
        self.cmd('git', 'remote', 'add', 'origin', str(self.remote))
        self.cmd('git', 'push', 'origin', 'main')
        self.branch = 'xnaut/runs/test-run'
        self.cmd('git', 'checkout', '-b', self.branch)
        self.artifacts = self.repo/'.xnaut/runs/test-run'
        self.artifacts.mkdir(parents=True)
        (self.artifacts/'.gitattributes').write_text('*.png filter=lfs diff=lfs merge=lfs -text\n*.mp4 filter=lfs diff=lfs merge=lfs -text\n')
        (self.repo/'.git/xnaut-transfer.json').write_text(json.dumps(dict(run_id='test-run', branch=self.branch, remote=str(self.remote), source_sha=self.sha, artifacts='.xnaut/runs/test-run')))
        shutil.copyfile(PUBLISHER, self.repo/'.git/xnaut-publish.py')

    def tearDown(self):
        self.tmp.cleanup()

    def cmd(self, *args, cwd=None, check=True):
        return subprocess.run(args, cwd=cwd or self.repo, capture_output=True, text=True, check=check,
                              env={**os.environ, 'PATH':str(self.root/'bin')+os.pathsep+os.environ['PATH'], 'GIT_CONFIG_GLOBAL':os.devnull, 'GIT_CONFIG_NOSYSTEM':'1', 'GIT_TERMINAL_PROMPT':'0'})

    def publish(self, check=True):
        return self.cmd('python3', '.git/xnaut-publish.py', '--finish', '0', check=check)

    def test_history_media_and_unrelated_staging(self):
        (self.artifacts/'report.md').write_text('Audit evidence')
        picture = b'\x89PNG\x00fake-test-image\n'
        video = b'fake-test-video\x00\xff'
        (self.artifacts/'screenshot.png').write_bytes(picture)
        (self.artifacts/'recording.mp4').write_bytes(video)
        (self.artifacts/'large.data').write_bytes(b'x' * (8 * 1024 * 1024))
        (self.repo/'private-unrelated.txt').write_text('must not be staged automatically')
        self.cmd('git', 'add', 'private-unrelated.txt')
        self.publish()
        self.assertEqual(self.cmd('git','--git-dir',str(self.remote),'rev-parse','main').stdout.strip(), self.sha)
        self.assertIn('version https://git-lfs.github.com/spec/v1', self.cmd('git','show','HEAD:.xnaut/runs/test-run/screenshot.png').stdout)
        self.assertNotEqual(self.cmd('git','cat-file','-e','HEAD:private-unrelated.txt',check=False).returncode,0)
        self.assertIn('A  private-unrelated.txt',self.cmd('git','status','--porcelain').stdout)
        checkout=self.root/'review'
        self.cmd('git','clone','-b',self.branch,str(self.remote),str(checkout))
        self.cmd('git','lfs','install','--local',cwd=checkout)
        self.cmd('git','lfs','pull',cwd=checkout)
        self.assertEqual((checkout/'.xnaut/runs/test-run/screenshot.png').read_bytes(),picture)
        self.assertEqual((checkout/'.xnaut/runs/test-run/recording.mp4').read_bytes(),video)
        self.assertEqual((checkout/'.xnaut/runs/test-run/large.data').stat().st_size,8 * 1024 * 1024)
        self.assertIn('version https://git-lfs.github.com/spec/v1', self.cmd('git','show','HEAD:.xnaut/runs/test-run/large.data').stdout)
        result=json.loads((checkout/'.xnaut/runs/test-run/result.json').read_text())
        self.assertTrue(result['uncommitted_source'])
        self.assertEqual(self.cmd('git','merge-base',self.sha,'HEAD',cwd=checkout).stdout.strip(),self.sha)

    def test_rejected_upload_is_retryable_and_does_not_duplicate_commit(self):
        hook=self.remote/'hooks/pre-receive'
        hook.write_text('#!/bin/sh\nexit 1\n'); hook.chmod(0o755)
        (self.artifacts/'note.md').write_text('Remember this')
        result=self.publish(check=False)
        self.assertNotEqual(result.returncode,0)
        self.assertEqual(json.loads((self.repo/'.git/xnaut-upload.json').read_text())['state'],'pending')
        detacher=json.loads((self.repo/'.git/test-tmux-launch.json').read_text())
        self.assertEqual(detacher[:4],['new-session','-d','-s','xnaut-upload-test-run'])
        self.assertIn('--retry',detacher[-1])
        committed=self.cmd('git','rev-parse','HEAD').stdout.strip()
        hook.unlink()
        self.publish()
        self.assertEqual(self.cmd('git','rev-parse','HEAD').stdout.strip(),committed)
        self.assertEqual(self.cmd('git','--git-dir',str(self.remote),'rev-parse',self.branch).stdout.strip(),committed)
        self.publish()
        self.assertEqual(self.cmd('git','rev-parse','HEAD').stdout.strip(),committed)

    def test_symlink_cannot_publish_outside_data(self):
        secret=self.root/'outside.txt';secret.write_text('outside')
        (self.artifacts/'outside.md').symlink_to(secret)
        self.assertNotEqual(self.publish(check=False).returncode,0)
        self.assertEqual(self.cmd('git','rev-parse','HEAD').stdout.strip(),self.sha)

    def test_changed_destination_or_branch_refused(self):
        self.cmd('git','checkout','main')
        self.assertNotEqual(self.publish(check=False).returncode,0)
        self.cmd('git','checkout',self.branch)
        self.cmd('git','remote','set-url','origin',str(self.root/'other.git'))
        self.assertNotEqual(self.publish(check=False).returncode,0)

if __name__ == '__main__':
    unittest.main()
