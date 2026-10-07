#!/usr/bin/env python3
"""Cooperative bounded control-repo maintenance; never prune/reset/expire.
Use instead of raw gc. Only POSIX flock is supported by this external helper;
Windows users must use the native maintenance scheduler's std::fs lease.
"""
import argparse
import os
from pathlib import Path
import subprocess
import sys

TASKS = ('loose-objects', 'commit-graph', 'incremental-repack', 'pack-refs')

def maintain(repo, task):
    if os.name != 'posix':
        raise RuntimeError('External lease unsupported on this OS; use native maintenance')
    import fcntl
    if task not in TASKS:
        raise ValueError('Only bounded Git maintenance tasks are supported')
    prefix = ['git', '-C', str(repo), '-c', 'gc.auto=0', '-c', 'maintenance.auto=false']
    env = dict(os.environ, GIT_OPTIONAL_LOCKS='0')
    common = Path(subprocess.check_output(prefix + ['rev-parse', '--path-format=absolute', '--git-common-dir'], text=True, env=env).strip()).resolve()
    with (common / 'xnaut-ticket-update.lock').open('a+b') as lease:
        try:
            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('A PM writer or maintenance process owns the lease; nothing ran') from None
        try:
            subprocess.run(prefix + ['-c', 'pack.threads=1', '-c', 'core.multiPackIndex=true', 'maintenance', 'run', '--quiet', '--task=' + task], env=env, check=True)
        finally:
            fcntl.flock(lease, fcntl.LOCK_UN)

if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo', type=Path, required=True)
    p.add_argument('--task', choices=TASKS, default='loose-objects')
    args = p.parse_args()
    try:
        maintain(args.repo, args.task)
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)
