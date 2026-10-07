# XNAUT-439 independent integration review

Integration base: c909b033900cd4576252189b5827fe2a8c1b13ae.

The author worktree on Tron and its branch were read only. The original handback
and its claimed 1,529 Rust / 399 browser results remain historical author evidence;
they are not independent validation of this integration.

Imported original commits f1f0e0297cc75b5e25eb13623fd8374f2aea2057 and
1c9b570f267470879cfd9798742442a3427a6334, preserving author attribution. The URL-test
change from 372a38f and ACL change from 24ca3d1 were already covered on the canonical
base. The later bundle-only commit 34988e3 adds no runtime fix.

Independent review found and corrected three integration blockers:

- Summed live-child CPU falls when compilers exit. A historical maximum then
  masked subsequent smaller, active children. Persisted PID/birth samples now
  accumulate positive per-child deltas; unknown samples preserve the baseline.
- Child CPU cannot prove progress when the recorded CLI PID/birth fails native
  liveness validation. The same sampled root must also match its recorded birth.
- Reviewer task completion recovery now recognizes both exact historical and
  current native stall reasons. Its publication, handback, identity and event
  history checks remain intact.

Regression coverage adds replacement-child churn across repeated manifest reloads,
PID reuse/unknown samples, an invalid recorded birth with a live viewport, and the
new native stall reason through the existing reviewer publication recovery fixture.
The existing real Unix child CPU fixture now supplies the required process birth.

Validation performed by the integrating agent: Rustfmt parser checks for both
changed Rust files and git diff --check. A separate agent reviewed the corrective
delta without finding a remaining concrete blocker. No Cargo, native app or owner
workspace mutation was performed. Parent-owned Rust/full native validation is
required before merge. Remote child CPU remains unavailable here, and Windows
without a compatible ps table relies on the other existing progress signals.
