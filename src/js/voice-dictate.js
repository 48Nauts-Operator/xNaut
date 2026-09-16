// Shared push-to-talk dictation (XNAUT-187). Lives here, not in chat-panel.js,
// because more than one composer needs it: the chat pane and the Agent Space
// composer are separate surfaces and the mic only ever existed in the first.
(() => {
  const invoke = (...a) => window.__TAURI__.core.invoke(...a);
  const session = window.xnautVoiceSession;
  let owner = null;
  let timeout = null;
  let observer = null;

  session.subscribe((state) => {
    if (!owner) return;
    owner.classList.toggle('is-recording', state.state === 'recording');
    owner.setAttribute('aria-pressed', String(state.state === 'recording'));
    if (state.state === 'error') owner.title = state.error;
    if (state.state === 'recording') owner.title = 'Click again to transcribe; Escape cancels';
    if (state.state === 'transcribing') owner.title = 'Transcribing… Escape cancels';
    if (state.state === 'idle') {
      clearTimeout(timeout);
      observer?.disconnect();
      observer = null;
      owner = null;
    }
  });

  const provider = {
    capabilities: { recognition: true, synthesis: false, streamingInput: false },
    async start(id, signal) {
      const ready = await invoke('voice_model_ready');
      signal.throwIfAborted();
      if (!ready && owner) owner.title = 'First transcription downloads the speech model (~148 MB)';
      await invoke('voice_start', { captureId: id });
      // IPC handlers can acquire their locks in a different order. A cancel
      // that reached Rust before start must also retire the late-started mic.
      if (signal.aborted) await invoke('voice_cancel', { captureId: id });
      return { notice: ready ? '' : 'First transcription downloads a speech model (~148 MB).' };
    },
    stop: (id) => invoke('voice_stop', { captureId: id }),
    cancel: (id) => invoke('voice_cancel', { captureId: id }),
  };

  const MIC_SVG = `<svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><rect x="5.25" y="2" width="5.5" height="7.5" rx="2.75"/><path d="M3.5 8a4.5 4.5 0 0 0 9 0M8 12.5V15M5.5 15h5"/></svg>`;

  // btn: the button element. onText: called with the transcript.
  function attachDictation(btn, onText, label = 'Message') {
    if (!btn || btn._dictationWired) return;
    btn._dictationWired = true;
    btn.setAttribute('aria-pressed', 'false');
    btn.onclick = async () => {
      if (owner === btn) {
        if (session.snapshot().state === 'recording') {
          clearTimeout(timeout);
          const epoch = await session.finishInput();
          if (epoch !== undefined) await session.end(epoch);
        } else {
          await session.end();
        }
        return;
      }
      if (session.snapshot().state !== 'idle') {
        btn.title = 'End the current voice session before changing destinations';
        return;
      }
      owner = btn;
      btn.title = 'Starting microphone…';
      observer = new MutationObserver(() => {
        if (!btn.isConnected) void session.end();
      });
      observer.observe(document.body, { childList: true, subtree: true });
      try {
        await session.begin({
          provider, mode: 'silent', onTranscript: onText,
          destination: { id: crypto.randomUUID(), label, connected: () => btn.isConnected },
        });
        if (owner !== btn || session.snapshot().state !== 'recording') return;
        // Also enforced by the native capture thread when the webview stalls.
        timeout = setTimeout(async () => {
          const epoch = await session.finishInput();
          if (epoch !== undefined) await session.end(epoch);
        }, 120000);
      } catch (e) {
        btn.title = String(e);
        console.error('[voice] cannot start dictation', e);
      }
    };
  }

  window.addEventListener('pagehide', () => { void session.end(); });
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && session.snapshot().state !== 'idle') {
      void session.end();
    }
  });

  // Append to a textarea the way a person would: a space between, cursor at
  // the end, and the caller's grow hook run so the box does not clip.
  function appendToInput(inputEl, text, grow) {
    inputEl.value = `${inputEl.value}${inputEl.value ? ' ' : ''}${text}`;
    inputEl.dispatchEvent(new Event('input', { bubbles: true }));
    if (typeof grow === 'function') grow(inputEl);
    inputEl.focus();
  }

  window.xnautAttachDictation = attachDictation;
  window.xnautDictationAppend = appendToInput;
  window.xnautDictationMicSvg = MIC_SVG;
})();
