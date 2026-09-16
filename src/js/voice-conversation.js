// XNAUT-420: optional local speech. Text submission still belongs to each
// conversation's existing send handler; no terminal scraping or second LLM.
(() => {
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const session = window.xnautVoiceSession;
  let active = null;
  let timer = null;
  let observer = null;

  function createLocalProvider() {
    let id, capture, generation = 0;
    let playback = Promise.resolve();
    async function record(signal) {
      await playback.catch(() => {});
      signal.throwIfAborted();
      capture = crypto.randomUUID();
      const currentCapture = capture;
      await invoke('voice_start', { captureId: currentCapture });
      if (signal.aborted) await invoke('voice_cancel', { captureId: currentCapture });
    }
    return {
      capabilities: { recognition: true, synthesis: true, streamingInput: false },
      async start(sessionId, signal) {
        id = sessionId;
        await invoke('voice_local_open', { sessionId: id });
        if (signal.aborted) {
          await invoke('voice_local_close', { sessionId: id });
          signal.throwIfAborted();
        }
        await record(signal);
        return { notice: 'Speech stays on this Mac; the selected agent may use cloud services. Completed chat replies are read aloud. Coding-session readback and spoken summaries are not connected yet.' };
      },
      record,
      async stop() {
        const currentCapture = capture;
        try { return await invoke('voice_local_transcribe', { sessionId: id, captureId: currentCapture }); }
        finally { if (capture === currentCapture) capture = null; }
      },
      async cancel(sessionId) {
        const currentCapture = capture;
        await Promise.all([
          currentCapture ? invoke('voice_cancel', { captureId: currentCapture }) : Promise.resolve(),
          invoke('voice_local_close', { sessionId }),
        ]);
        capture = null;
      },
      speak(text, signal) {
        const currentGeneration = ++generation;
        const interrupt = () => {
          void invoke('voice_local_interrupt', { sessionId: id, generation: currentGeneration }).catch(() => {});
        };
        signal.addEventListener('abort', interrupt, { once: true });
        const job = playback.catch(() => {}).then(async () => {
          if (signal.aborted) return;
          await invoke('voice_local_speak', { sessionId: id, generation: currentGeneration, text });
        }).finally(() => signal.removeEventListener('abort', interrupt));
        playback = job;
        return job;
      },
    };
  }

  // Full reads presentation prose. Code remains visible rather than being
  // voiced as implementation payloads; the overlay discloses this policy.
  function speechChunks(text) {
    // Unknown action envelopes can bypass a pane's known-action dispatcher.
    // Structured payloads are still not assistant prose, even when displayed.
    const raw = String(text || '').trim();
    if (/^[{[]/.test(raw) || /^<tool[_-]?call/i.test(raw)) return [];
    const plain = String(text || '').replace(/```[\s\S]*?```/g, ' Code block shown in the written answer. ')
      .replace(/~~~[\s\S]*?~~~/g, ' Code block shown in the written answer. ')
      .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
      .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
      .replace(/^\s{0,3}#{1,6}\s+/gm, '').replace(/[*_`]/g, '').trim();
    const chunks = [];
    let remaining = [...plain];
    while (remaining.length) {
      let size = Math.min(480, remaining.length);
      if (size < remaining.length) {
        const candidate = remaining.slice(0, size).join('');
        const boundary = candidate.lastIndexOf(' ');
        if (boundary > 100) size = [...candidate.slice(0, boundary)].length;
      }
      const chunk = remaining.splice(0, size).join('').trim();
      if (chunk) chunks.push(chunk);
    }
    return chunks;
  }

  session.subscribe((state) => {
    if (state.persistent && state.state === 'recording') {
      if (!timer) timer = setTimeout(() => {
        timer = null;
        // The recording limit creates a draft, never an automatic agent task.
        void session.finishInput({ submit: false });
      }, 120000);
    } else {
      clearTimeout(timer);
      timer = null;
    }
    if (state.state === 'idle') {
      if (active) active.button.setAttribute('aria-pressed', 'false');
      active = null;
      observer?.disconnect();
      observer = null;
    }
  });

  function attach(microphoneButton, { label, insert, submit, connected }) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'voice-conversation-button';
    button.textContent = 'Talk';
    button.title = 'Local voice conversation · requires the optional speech service';
    button.setAttribute('aria-label', `Talk to ${label}`);
    button.setAttribute('aria-pressed', 'false');
    microphoneButton.insertAdjacentElement('afterend', button);
    const destination = { id: crypto.randomUUID(), label, connected };
    const surface = { button, destination };
    button.onclick = async () => {
      if (active === surface) { await session.end(); return; }
      if (session.snapshot().state !== 'idle') {
        button.title = 'End the current voice session before changing destinations';
        return;
      }
      active = surface;
      button.setAttribute('aria-pressed', 'true');
      observer = new MutationObserver(() => { if (!connected()) void session.end(); });
      observer.observe(document.body, { childList: true, subtree: true });
      try {
        await session.begin({
          destination, provider: createLocalProvider(), mode: 'full', persistent: true,
          onTranscript: async (text, options) => { insert(text); if (options.submit) await submit(); },
        });
      } catch (error) { button.title = String(error); }
    };
    return {
      end() { if (active === surface) return session.end(); },
      beginTurn(turnId) {
        if (active !== surface || session.snapshot().state !== 'ready') return null;
        return session.beginTurn(turnId);
      },
      reply(token, text) {
        if (!token || active !== surface) return;
        speechChunks(text).forEach((chunk, index) => {
          session.offer(token, { id: `answer-${index}`, kind: 'answer', text: chunk });
        });
      },
      finishTurn: (token) => session.finishTurn(token),
    };
  }
  window.xnautAttachVoiceConversation = attach;
})();
