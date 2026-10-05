#!/usr/bin/env python3
"""Seed the full native acceptance build without touching the running profile."""

from pathlib import Path
import os
import shutil
import sqlite3
import sys

if sys.platform != "darwin":
    raise SystemExit("This local preview profile helper currently supports macOS.")
source = Path.home() / "Library/Application Support/xnaut"
target = source / (
    "continuity-preview"
    if "--continuity" in sys.argv
    else "journal-preview-3"
    if "--journal" in sys.argv
    else "full-wiki-preview"
)
# Preserve the owner's latest Preview 2 conversations without sharing its DB.
if (
    "--journal" in sys.argv
    and "--continuity" not in sys.argv
    and (source / "journal-preview/conversations.sqlite").exists()
):
    source = source / "journal-preview"
target.mkdir(mode=0o700, parents=True, exist_ok=True)
os.chmod(target, 0o700)
settings = target / "settings.json"
if not settings.exists() and (source / "settings.json").exists():
    shutil.copyfile(source / "settings.json", settings)
    os.chmod(settings, 0o600)
database = target / "conversations.sqlite"
if not database.exists() and (source / "conversations.sqlite").exists():
    with sqlite3.connect(
        (source / "conversations.sqlite").as_uri() + "?mode=ro", uri=True
    ) as old:
        with sqlite3.connect(database) as new:
            old.backup(new)
    os.chmod(database, 0o600)
print("Full preview has separate settings and conversation storage.")
