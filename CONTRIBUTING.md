# Contributing to xNAUT

Thanks for wanting to help. A few things about this project are unusual, so
please read this before opening a PR.

## Where development happens

xNAUT is developed on a private Forgejo instance. This GitHub repository
carries releases, issues, and discussions; the `main` branch mirrors the
released state, not day-to-day development.

What that means for you:

- **Bug reports and feature requests**: open a GitHub issue. That is the
  best way to help and it reaches us directly.
- **Questions and ideas**: use GitHub Discussions.
- **Pull requests**: welcome against `main`. A maintainer reviews them and
  ports accepted changes onto the internal release lineage; your commit is
  preserved with authorship, or credited in the file header if a rewrite is
  needed. Small, focused PRs have the best chance.

## Ground rules

- English for code, comments, and commit messages.
- Every borrowed mechanism names its source in the file header (project,
  file or symbol, license). We build by reading other people's solutions and
  we credit them; PRs that copy code without attribution are not merged.
- No secrets in diffs, ever.
- Match the style of the file you are editing.

## Building locally

```
cd src-tauri
cargo tauri dev        # development with live frontend
cargo test             # test suite
```

macOS needs Xcode command line tools; Linux needs the WebKit/GTK packages
listed in the README.

## Reporting a bug well

Include the app version (bottom-left in the status bar), your OS and
version, exact steps, and what you expected against what happened. The
issue template asks for all of this; please fill it in. The panic log at
`~/Library/Application Support/xnaut/rust-panics.log` (macOS) is often the
missing piece.
