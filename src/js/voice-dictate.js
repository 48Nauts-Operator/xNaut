// Shared push-to-talk dictation (XNAUT-187). Lives here, not in chat-panel.js,
// because more than one composer needs it: the chat pane and the Agent Space
// composer are separate surfaces and the mic only ever existed in the first.
(() => {
  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  const MIC_SVG = `<svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><rect x="5.25" y="2" width="5.5" height="7.5" rx="2.75"/><path d="M3.5 8a4.5 4.5 0 0 0 9 0M8 12.5V15M5.5 15h5"/></svg>`;

  // btn: the button element. onText: called with the transcript.
  function attachDictation(btn, onText) {
    if (!btn || btn._dictationWired) return;
    btn._dictationWired = true;
    let recording = false;
    const say = (msg) => { btn.title = msg; };
    btn.onclick = async () => {
      if (recording) {
        recording = false;
        btn.classList.remove('is-recording');
        say('Transcribing…');
        try {
          const { text } = await invoke('voice_stop');
          if (text) onText(text);
          say('Dictate message');
        } catch (e) {
          say(String(e));
          console.error('[voice] dictation failed', e);
        }
        return;
      }
      // First run downloads the speech model; saying so beats a button that
      // looks stuck for a minute and a half.
      try {
        const ready = await invoke('voice_model_ready');
        if (!ready) say('First use downloads the speech model (~148 MB)');
      } catch (_) { /* the start call below reports anything real */ }
      try {
        await invoke('voice_start');
        recording = true;
        btn.classList.add('is-recording');
        say('Click again to stop and transcribe');
      } catch (e) {
        say(String(e));
        console.error('[voice] cannot start dictation', e);
      }
    };
  }

  // Append to a textarea the way a person would: a space between, cursor at
  // the end, and the caller's grow hook run so the box does not clip.
  function appendToInput(inputEl, text, grow) {
    inputEl.value = `${inputEl.value}${inputEl.value ? ' ' : ''}${text}`;
    if (typeof grow === 'function') grow(inputEl);
    inputEl.focus();
  }

  window.xnautAttachDictation = attachDictation;
  window.xnautDictationAppend = appendToInput;
  window.xnautDictationMicSvg = MIC_SVG;
})();
