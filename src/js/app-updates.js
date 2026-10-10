// XNAUT-481: one update lifecycle, independent of whether its panel is open.
// Uses tauri-plugin-updater's separate download/install API. Its Finished event
// precedes signature verification; only download() resolving means ready.
// Windows install() exits the process, so consent and persistence come first.
(function () {
  'use strict';
  const RELEASE = 'https://github.com/48Nauts-Operator/xNaut/releases/latest';
  const FEED = 'https://github.com/48Nauts-Operator/xNaut/releases/latest/download/latest.json';
  const CHECK_INTERVAL = 6 * 60 * 60 * 1000;
  const RETRY_INTERVAL = 15 * 60 * 1000;
  const CHECK_TIMEOUT = 20000;
  const DOWNLOAD_TIMEOUT = 15 * 60 * 1000;
  const PREF_KEY = 'xnaut-updates:v1';
  let prefs = {};
  try { prefs = JSON.parse(localStorage.getItem(PREF_KEY)) || {}; } catch (_) { /* defaults */ }
  let started = false, panel, previousFocus, renderPending = false, nextCheck = 0;
  let phase = 'idle', update = null, current = '', version = '', notes = '', error = '';
  let checkedAt = 0, busy = false, received = 0, total = 0, downloadStarted = false;
  let finished = false, stalled = false, lastProgress = 0;

  function savePrefs() {
    try { localStorage.setItem(PREF_KEY, JSON.stringify(prefs)); }
    catch (e) { console.warn('[updater] preferences could not be saved:', e); }
  }
  const stableVersion = value => /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\+[\w.-]+)?$/.exec(String(value));
  function newer(a, b) {
    // The public channel is stable. Do not offer an unparseable version or a
    // prerelease to a stable install; the native plugin also checks versions.
    const av = stableVersion(a), bv = stableVersion(b);
    if (!av || !bv) return false;
    for (let i = 1; i <= 3; i++) {
      if (Number(av[i]) !== Number(bv[i])) return Number(av[i]) > Number(bv[i]);
    }
    return false;
  }
  async function closeResource(value) {
    try { await value?.close?.(); }
    catch (e) { console.warn('[updater] could not release update resource:', e); }
  }
  function message() {
    const mb = bytes => `${(bytes / 1048576).toFixed(1)} MB`;
    switch (phase) {
      case 'checking': return 'Checking for updates…';
      case 'current': return 'You’re up to date.';
      case 'available': return `xNAUT ${version} is available.`;
      case 'downloading':
        if (finished) return 'Verifying the downloaded update…';
        if (stalled) return downloadStarted ? 'Download stalled. Waiting for the connection to recover…' : 'No response yet. Waiting for the download server…';
        if (!downloadStarted) return 'Connecting to the download server…';
        return total ? `Downloading ${mb(received)} / ${mb(total)}` : `Downloading ${mb(received)}`;
      case 'ready': return `xNAUT ${version} is downloaded and verified. Ready when you are.`;
      case 'saving': return 'Checking saved conversations before restarting…';
      case 'installing': return `Installing xNAUT ${version}…`;
      case 'restarting': return 'Restarting xNAUT…';
      case 'installed': return `xNAUT ${version} is installed. Quit and reopen xNAUT to finish.`;
      case 'disabled': return 'Updates are disabled for development and preview builds.';
      case 'check-error': return `Could not check for updates: ${error}`;
      case 'download-error': return `Download or verification failed: ${error}`;
      case 'install-error': return `Installation failed: ${error}`;
      case 'save-error': return `Restart paused: ${error}`;
      case 'restart-error': return `Update installed; restart failed: ${error}. Quit and reopen xNAUT, or retry.`;
      default: return 'Check for the latest stable release of xNAUT.';
    }
  }
  function isStaged() {
    return ['ready', 'save-error', 'install-error', 'saving', 'installing', 'installed', 'restarting', 'restart-error'].includes(phase);
  }
  function render() {
    renderPending = false;
    const chip = document.getElementById('btn-updates');
    const snoozed = prefs.snoozeVersion === version && Number(prefs.snoozeUntil) > Date.now();
    if (chip) {
      chip.hidden = !version || (snoozed && phase === 'available');
      chip.textContent = busy && phase !== 'checking' ? 'Updating…' : isStaged() ? 'Update ready' : /error$/.test(phase) ? 'Update needs attention' : 'Update available';
      chip.title = message();
    }
    if (!panel) return;
    panel.querySelector('[data-version]').textContent = current ? `Installed version ${current}` : 'Installed version unavailable';
    panel.querySelector('[data-status]').textContent = message();
    panel.querySelector('[data-checked]').textContent = checkedAt ? `Last checked ${new Date(checkedAt).toLocaleString()}` : '';
    panel.querySelector('[data-notes]').textContent = notes || 'Release notes are available on the release page.';
    panel.querySelector('details').hidden = !version;
    const progress = panel.querySelector('progress');
    progress.hidden = phase !== 'downloading';
    if (total && !finished) { progress.max = total; progress.value = Math.min(total, received); }
    else progress.removeAttribute('value');
    const action = panel.querySelector('[data-primary]');
    action.hidden = !version || ['installed', 'restarting'].includes(phase);
    action.disabled = busy;
    action.textContent = phase === 'download-error' ? 'Retry download'
      : ['ready', 'save-error', 'install-error'].includes(phase) ? 'Install and restart now'
      : phase === 'restart-error' ? 'Retry restart'
      : busy ? 'Please wait…'
      : update ? 'Download update' : 'Download from website';
    panel.querySelector('[data-check]').disabled = busy || isStaged() || phase === 'disabled';
    panel.querySelector('[data-snooze]').hidden = phase !== 'available';
    panel.querySelector('[data-auto]').checked = prefs.auto !== false;
    panel.querySelector('[data-auto]').disabled = phase === 'disabled';
    panel.querySelector('[data-restart-note]').hidden = !isStaged();
  }
  function renderProgress() {
    if (!renderPending) { renderPending = true; requestAnimationFrame(render); }
  }
  function setPhase(value) { phase = value; render(); }
  async function manualDownload() {
    try {
      if (window.__TAURI__?.shell?.open) await window.__TAURI__.shell.open(RELEASE);
      else window.open(RELEASE, '_blank', 'noopener,noreferrer');
    } catch (e) {
      error = String(e?.message || e);
      panel.querySelector('[data-status]').textContent = `Could not open the release page: ${error}`;
    }
  }
  function open() {
    if (!panel) {
      panel = document.createElement('dialog');
      panel.id = 'update-panel';
      panel.setAttribute('aria-labelledby', 'update-title');
      panel.innerHTML = `
        <div class="update-heading"><div><p class="update-eyebrow">xNAUT</p><h2 id="update-title">Updates</h2></div><button data-close aria-label="Close updates">×</button></div>
        <p data-version class="update-muted"></p>
        <p data-status role="status" aria-live="polite"></p>
        <progress aria-label="Update download progress" hidden></progress>
        <p data-restart-note class="update-notice" hidden>Finish active tasks and save open edits before installing. Installation restarts xNAUT and can interrupt local terminals and agents. Downloads never restart the app.</p>
        <details><summary>What’s new</summary><pre data-notes></pre></details>
        <div class="update-actions"><button data-primary class="update-primary" hidden>Download update</button><button data-snooze hidden>Remind me tomorrow</button></div>
        <div class="update-preferences"><label><input type="checkbox" data-auto> Automatically check for updates</label><p class="update-muted">Checks every six hours while xNAUT is open. Downloads start only when you choose.</p></div>
        <p data-checked class="update-muted"></p>
        <div class="update-actions"><button data-check>Check now</button><button data-release>Release page</button></div>`;
      document.body.appendChild(panel);
      panel.querySelector('[data-close]').onclick = () => panel.close();
      panel.addEventListener('close', () => { previousFocus?.focus?.(); });
      panel.querySelector('[data-check]').onclick = () => check(true);
      panel.querySelector('[data-release]').onclick = manualDownload;
      panel.querySelector('[data-snooze]').onclick = () => {
        prefs.snoozeVersion = version; prefs.snoozeUntil = Date.now() + 24 * 60 * 60 * 1000;
        savePrefs(); render(); panel.close();
      };
      panel.querySelector('[data-auto]').onchange = event => {
        prefs.auto = event.target.checked; savePrefs();
        if (prefs.auto) check(false);
      };
      panel.querySelector('[data-primary]').onclick = () => {
        if (busy) return;
        if (phase === 'restart-error') restart();
        else if (['ready', 'save-error', 'install-error'].includes(phase)) install();
        else if (update) download();
        else manualDownload();
      };
    }
    render();
    if (!panel.open) {
      const focused = document.activeElement;
      previousFocus = focused !== document.body && focused?.getClientRects().length
        ? focused : document.getElementById('btn-more-menu');
      panel.showModal();
      panel.querySelector('[data-close]').focus();
    }
    if (['idle', 'check-error'].includes(phase)) check(true);
  }
  async function check(manual = false) {
    if (busy || isStaged() || phase === 'disabled') return;
    if (!manual && (prefs.auto === false || Date.now() < nextCheck || document.hidden)) return;
    if (!window.__TAURI__) return;
    busy = true;
    setPhase('checking');
    let candidate;
    try {
      const resource = await window.__TAURI__.path?.resourceDir?.();
      if (window.xnautFullWikiPreview || /worktrees|target[\/\\](?:debug|release)/.test(String(resource))) {
        setPhase('disabled'); return;
      }
      current = await window.__TAURI__.app?.getVersion?.();
      if (!current) throw new Error('The installed version could not be determined');
      if (!stableVersion(current)) throw new Error('This build is outside the stable update channel');
      if (navigator.onLine === false) throw new Error('You are offline. Checks resume when you reconnect.');
      if (window.__TAURI__.updater?.check) {
        candidate = await window.__TAURI__.updater.check({ timeout: CHECK_TIMEOUT });
      } else {
        // Same release manifest as the native updater. Without the plugin we
        // can announce a version, but must not claim to download or verify it.
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), CHECK_TIMEOUT);
        try {
          const response = await fetch(FEED, { signal: controller.signal, cache: 'no-store' });
          if (!response.ok) throw new Error(`Release server returned HTTP ${response.status}`);
          candidate = await response.json();
        } finally { clearTimeout(timer); }
      }
      if (candidate && !stableVersion(candidate.version)) throw new Error('The release server returned an invalid stable version');
      // Do not discard a previously discovered update on a transient failure.
      await closeResource(update);
      update = null; version = ''; notes = ''; error = '';
      if (candidate && newer(candidate.version, current)) {
        version = candidate.version.replace(/^v/, ''); notes = String(candidate.body || candidate.notes || '');
        if (typeof candidate.download === 'function' && typeof candidate.install === 'function') {
          update = candidate; candidate = null;
        }
      }
      checkedAt = Date.now(); nextCheck = checkedAt + CHECK_INTERVAL;
      setPhase(version ? 'available' : 'current');
    } catch (e) {
      error = String(e?.message || e); nextCheck = Date.now() + RETRY_INTERVAL;
      console.warn('[updater] check failed:', e);
      setPhase('check-error');
    } finally {
      await closeResource(candidate);
      busy = false; render();
    }
  }
  async function download() {
    if (busy || !update || isStaged()) return;
    busy = true; received = 0; total = 0; downloadStarted = false;
    finished = false; stalled = false; lastProgress = Date.now(); error = '';
    setPhase('downloading');
    const stallMs = Number(window.xnautUpdateStallMs) || 90000;
    const watchdog = setInterval(() => {
      if (!finished && !stalled && Date.now() - lastProgress >= stallMs) {
        stalled = true; render();
        console.warn(`[updater] download stalled after ${received} bytes; awaiting actual result`);
      }
    }, Math.max(200, Math.min(1000, stallMs)));
    try {
      await update.download(event => {
        if (event?.event === 'Started') {
          downloadStarted = true; total = Number(event.data?.contentLength) || 0;
        } else if (event?.event === 'Progress') {
          downloadStarted = true; received += Number(event.data?.chunkLength) || 0;
        } else if (event?.event === 'Finished') finished = true;
        else return;
        stalled = false; lastProgress = Date.now(); renderProgress();
      }, { timeout: DOWNLOAD_TIMEOUT });
      setPhase('ready');
    } catch (e) {
      error = String(e?.message || e);
      console.error(`[updater] download/verification failed after ${received} bytes:`, e);
      setPhase('download-error');
    } finally { clearInterval(watchdog); busy = false; render(); }
  }
  async function confirmSaved() {
    if (typeof window.xnautConversationStorage?.confirmAllSaved !== 'function') {
      throw new Error('Conversation storage is unavailable');
    }
    let timer;
    try {
      // Read-only wait: timing out does not start an installation or cancel a
      // write. The queue keeps its actual result and a retry checks it again.
      await Promise.race([
        window.xnautConversationStorage.confirmAllSaved(),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Saving conversations is taking longer than expected. Try again after saving finishes.')), 10000); }),
      ]);
    } finally { clearTimeout(timer); }
  }
  async function install() {
    if (busy || !update) return;
    busy = true; setPhase('saving');
    try { await confirmSaved(); }
    catch (e) { error = String(e?.message || e); busy = false; setPhase('save-error'); return; }
    setPhase('installing');
    try {
      await update.install();
      // On Windows the installer takes over. On macOS/Linux we relaunch only
      // after installation returns, under the same explicit restart click.
      setPhase('installed');
    } catch (e) {
      error = String(e?.message || e); busy = false;
      console.error('[updater] install failed:', e); setPhase('install-error'); return;
    }
    await closeResource(update); update = null;
    busy = false;
    await restart();
  }
  async function restart(checkStorage = true) {
    if (busy) return;
    if (!window.__TAURI__?.process?.relaunch) { setPhase('installed'); return; }
    busy = true;
    try {
      if (checkStorage) await confirmSaved();
      setPhase('restarting');
      await window.__TAURI__.process.relaunch();
    } catch (e) {
      error = String(e?.message || e); console.error('[updater] restart failed:', e);
      setPhase('restart-error');
    } finally { busy = false; render(); }
  }
  function start() {
    if (started) return;
    started = true;
    document.getElementById('btn-updates')?.addEventListener('click', open);
    document.querySelector('[data-action="updates"]')?.addEventListener('keydown', event => {
      if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); event.currentTarget.click(); }
    });
    setTimeout(() => check(false), 3000);
    setInterval(() => { render(); check(false); }, 60000);
    window.addEventListener('focus', () => check(false));
    document.addEventListener('visibilitychange', () => { if (!document.hidden) check(false); });
    window.addEventListener('online', () => { nextCheck = 0; check(false); });
  }
  window.xnautUpdates = { start, open };
})();
