# Designer: a fresh sandbox never serves, and the UI sits on "Starting the sandbox…"

## RESOLVED. Root cause below; the rest of this file is the trail that led there.

**`gitvm run` forced `ssh -t`. xNAUT launches it with no controlling terminal, so
OpenSSH refused to allocate a PTY and exited 255 before the remote command ran.
xNAUT then ignored that exit status and probed an empty port for seven minutes.**

Fixes:

* `GitVM/cli/gitvm` requests a PTY only when stdin and stdout are both TTYs, and
  surfaces rsync/chown failures.
* `sandbox.rs` adds checked execution, so a non-zero `gitvm run` can no longer
  become a false success.
* `designer.rs` verifies the holding/dev server actually binds its port, and the
  per-step timeout helper is now active across adoption, spin-up, publish,
  renewal, checkpointing, cleanup and stop.

Verified on an empty sandbox: `/tmp/designer-holding` created, port 3000 bound,
public URL 200, exit codes propagating, 314 tests passing.

**And the reason I could not find it: every experiment I ran had a terminal.**
Running the identical command from a shell always worked, because a shell has a
TTY. The one condition that mattered was the one my testing could not reproduce.
The port override I called "most likely" was also wrong; the control plane
confirmed `exposedPort: 3000`.

---

Original brief follows.

Repo: `xnaut`, worktree `.worktrees/nautflow-incident-loop`, branch
`feature/loops-platform`. Relevant files: `src-tauri/src/designer.rs`,
`src-tauri/src/sandbox.rs` (the `cli` module, aliased `gvm`).

## Symptom

Create a new design in the Designer. The UI shows `Starting the sandbox…` and
stays there. Sometimes it later shows
`Sandbox did not answer — destroying it and retrying…`. The canvas never loads.

## What is definitely true

Measured on live sandboxes, several times today.

1. **The sandbox is created and healthy.** `gitvm status` reports `running` with
   hours left on the lease. The control plane knows it.
2. **Nothing listens on the exposed port (3000).** Inside the VM, `ss -ltn`
   shows only `6080` (noVNC) and `5900` (VNC). On one box `8090` was also there,
   from `expose_local_port`, which proves that step ran.
3. **`/tmp/designer-holding` does not exist.** That directory is created by the
   `else` branch of `ensure_dev_server`, so that branch never executed.
4. **The public URL returns 502.** Correct behaviour: the tunnel is up, nothing
   is listening behind it.
5. **`/workspace` contains only `.gitignore`, `.gitvm.json`, `design.json`.** No
   `package.json`, which is expected on a brand-new design; the agent has not
   written anything yet. So `ensure_dev_server` should take its `else` branch and
   start the python holding page.
6. **Running that holding-page command by hand WORKS.** Same sandbox, same port:

   ```
   gitvm run '(setsid python3 -m http.server 3000 --bind 0.0.0.0 </dev/null >/tmp/b.log 2>&1 &); sleep 2; echo started-3000'
   ```
   Returns in 6s, port binds, and the public URL goes 502 → **200**.

So the mechanism is sound and something upstream is not reaching it.

## Hypotheses I tested and DISPROVED

Please do not spend time on these; each was checked directly.

- **Nested single quotes in the script break `gitvm run`.** No.
  `gitvm run "printf '%s' 'hello' > /tmp/t2 && cat /tmp/t2"` returns in 3s.
- **Binding the *exposed* port specifically hangs the call.** No. Binding 3000
  and binding an unexposed 3999 both return in 6s.
- **`gitvm run` hangs when the command backgrounds a daemon.** No.
  `(setsid sleep 60 ... &)` returns in 4s.

I did twice observe a `gitvm run` invocation of mine exceed 120 seconds, but I
could not reproduce it deliberately afterwards, and the two commands differed
only in ways the tests above rule out. Treat that as unexplained rather than as
evidence.

## The code path

From the code graph (`trace_path ensure_dev_server`):

```
designer_spin_up  ──┐
                    ├──> ensure_dev_server ──> gvm::run ──> gitvm run <script>
designer_publish  ──┘
```

`designer_spin_up`, inside one `spawn_blocking`, in order:

1. `gvm::state_is_stale(&d)` → `gvm::stop` only if the control plane 404s the id
2. `gvm::warm_up(&d)?`
3. `gvm::expose_local_port(&d, 8090)`, whose errors are logged but not fatal
4. `gvm::public_url(&d)?`
5. `ensure_dev_server(&d, port)?`   ← the step whose effects never appear
6. then a probe loop: 2xx/3xx wins; 502–504 waits up to `PROBE_WAIT_SECS` (420);
   anything else destroys and retries, twice total

## What I want checked

1. **Is step 5 reached at all on a fresh design?** If `warm_up` returns `Err`,
   the `?` exits the closure and nothing after it runs. On a design that already
   has a live box, `gitvm warm-up` answers `already warm — sandbox <slug> is running.
   'gitvm stop' first.`, which is an error and would explain a wedged design.
   But on a genuinely NEW design there is no prior state, so warm-up should
   succeed. Confirm which.
2. **Does `ensure_dev_server` return `Ok` while having done nothing?** Its result
   is a string from the remote shell. If the script's `if [ -f package.json ]`
   test or the `else` branch silently fails inside the VM, the function can still
   return `Ok("")` and the caller would proceed to probe a port nothing binds.
3. **Does `gvm::run` rsync before running, and can that fail quietly?**
   `gitvm run` syncs the local dir into `/workspace` first. If the sync fails or
   runs as the wrong user, the subsequent command may execute somewhere
   unexpected.
4. **Is the exposed port actually 3000 for this template?** `write_gitvm_config`
   deliberately overrides the `agent-desktop` manifest, whose own port is 6080
   (noVNC). If the override is not applied to the created VM, the tunnel and the
   holding page would be on different ports and every probe would 502 forever.
   This one feels most likely to me and I ran out of road before testing it.

## What a fix has to satisfy

- A brand-new design shows something in the canvas within ~30s, even with an
  empty workspace. A holding page is fine; a spinner is not.
- No step may block indefinitely. I have added `timed()` in `designer.rs` which
  deadlines each sandbox call and logs both ends, but the underlying step still
  needs to not hang.
- Failures must name the step. Until today the only signal was one overwritten
  status string, which is why this took a whole afternoon to characterise.

## What I changed, and what it does NOT fix

Two things happened today that will mislead you if you find them unannounced.

### 1. A code fix for a DIFFERENT failure (commit `2038b20`)

There is a second, distinct bug that I did fix. A design can end up with
`design.json` holding `sandbox_id: ""` while `.gitvm/state.json` still holds a
healthy sandbox. That combination is permanent:

- `is_live()` reads `design.json`, sees no sandbox, and will not reconnect
- `gitvm warm-up` refuses, because state.json exists and the control plane agrees
  the box is running: `already warm — sandbox <slug> is running. 'gitvm stop' first.`

so both attempts fail forever and `ensure_dev_server` is never reached. Three
designs were in that state. `designer_spin_up` now ADOPTS such a sandbox, but
only if the control plane does not 404 it AND its URL answers 2xx/3xx, and it
reads the lease from the server's `expiresAt` rather than assuming a fresh one.

**This is not the bug in this document.** It rescues a design whose sandbox
already works. A brand-new design has no state file to adopt, so adoption never
runs and the fresh-creation path is untouched. I initially reported this as "the
Designer is fixed", which was wrong.

The same commit also stops `designer_spin_up` swallowing a failed teardown, since
a destroy that fails while `design.json` was never written is what creates the
wedge in the first place.

### 2. A MANUAL rescue of one live sandbox, by hand, outside the app

`guardian-website-3` / `solid-deer-9786` is currently serving HTTP 200. **The app
did not do that. I did**, from a shell, to unblock a screen recording:

```
cd ~/.xnaut-vault/work/Guardian/Design/guardian-website-3
gitvm run '(setsid python3 -m http.server 3000 --bind 0.0.0.0 </dev/null >/tmp/b.log 2>&1 &); sleep 2; echo started-3000'
```

That is the `else` branch of `ensure_dev_server`, run manually. It bound port
3000 in about six seconds and the public URL went 502 → 200.

Two consequences for anyone investigating:

- **That box is no longer a clean sample.** Its port is bound, `/tmp/dev.log`
  exists, and it looks like a success. It is not one. For a true reproduction,
  create a NEW design.
- **It is also the strongest evidence available**: the exact command the app is
  supposed to run works on the exact machine the app is supposed to run it on,
  at the exact port. Whatever is broken is between `designer_spin_up` deciding to
  call `ensure_dev_server` and that command actually executing in the VM.

Any sandbox with `/tmp/designer-holding` present but created by hand should be
treated as contaminated. A genuinely untouched failure has no such directory,
which is finding #3 above.

## Instrumentation already added (commit `ceaf906`)

`designer.rs` now writes a durable JSONL log per design to
`~/Library/Application Support/xnaut/looms/logs/design-<slug>.jsonl`, containing
every status line, `ensure_dev_server`'s return value, every probe result with
its HTTP code, and a duration for each sandbox call. **Reproducing once with this
build will likely answer questions 1 and 2 immediately.** It requires an app
restart to load.

## Reproduce

```
# in the app: Designer → new design → type any prompt
# then, from the design's folder:
cd ~/.xnaut-vault/work/<Project>/Design/<slug>
gitvm status
gitvm run 'ss -ltn; ls -A /workspace; ls /tmp/designer-holding'
curl -o /dev/null -w '%{http_code}\n' "$(python3 -c "import json;print(json.load(open('.gitvm/state.json'))['publicUrl'])")"
```
