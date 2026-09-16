// XNAUT-418: shared voice lifecycle. Providers own audio, surfaces own text and
// submission. No focus lookup, automatic send, provider fallback or summarizer.
(() => {
  const MODES = new Set(['full', 'summary', 'silent']);
  const MAX_QUEUE_BYTES = 32 * 1024;
  const MAX_SEGMENTS = 512;

  function createCoordinator() {
    let session = null;
    let epoch = 0;
    let cleanup = Promise.resolve();
    let retirementError = '';
    const listeners = new Set();
    const bytes = (text) => new TextEncoder().encode(text).length;
    const snapshot = () => session ? {
      epoch: session.epoch, destination: session.destination.label,
      state: session.state, mode: session.mode, error: session.error,
      notice: session.notice || '',
      speaking: session.speaking, queued: session.queue.length,
    } : retirementError ? {
      state: 'error', destination: 'Voice session', error: retirementError,
    } : { state: 'idle' };
    function notify() { for (const fn of listeners) fn(snapshot()); }
    const current = (s) => session === s;
    function stopPlayback(s) {
      s.generation++;
      s.playback.abort();
      s.playback = new AbortController();
      s.queue = [];
      s.queueBytes = 0;
      s.speaking = false;
    }
    function end(expectedEpoch) {
      const s = session;
      if (expectedEpoch !== undefined && s?.epoch !== expectedEpoch) return Promise.resolve();
      if (!s) {
        retirementError = '';
        notify();
        return cleanup;
      }
      session = null;
      s.input.abort();
      stopPlayback(s);
      // Providers must cancel even while start() is still awaiting permission.
      // Serialize retirement before a subsequent provider may claim the mic.
      cleanup = Promise.allSettled([s.starting, Promise.resolve().then(() => s.provider.cancel(s.id))])
        .then((results) => {
          if (results[1].status === 'rejected') {
            retirementError = `Could not release voice input: ${results[1].reason}`;
            if (session) { session.error = retirementError; session.state = 'error'; }
            notify();
          }
        });
      notify();
      return cleanup;
    }
    async function begin({ destination, provider, onTranscript, mode = 'silent' }) {
      if (session) throw new Error('End the current voice session first');
      if (!destination?.id || !destination.label || typeof onTranscript !== 'function') {
        throw new Error('A pinned voice destination is required');
      }
      if (!MODES.has(mode) || (mode !== 'silent' && !provider.capabilities.synthesis)) {
        throw new Error('This voice provider does not support that playback mode');
      }
      const s = {
        id: crypto.randomUUID(), epoch: ++epoch, destination, provider, onTranscript,
        mode, state: 'starting', input: new AbortController(),
        playback: new AbortController(), generation: 0, turnId: null,
        queue: [], queueBytes: 0, seen: new Set(), speaking: false, error: '',
      };
      session = s;
      notify();
      s.starting = (async () => {
        await cleanup;
        if (!current(s)) return;
        if (retirementError) throw new Error(retirementError);
        const ready = await provider.start(s.id, s.input.signal);
        if (!current(s)) return;
        s.notice = ready?.notice || '';
        s.state = 'recording';
        notify();
      })();
      try { await s.starting; }
      catch (error) {
        if (current(s)) {
          s.error = String(error);
          s.state = 'error';
          s.input.abort();
          await Promise.resolve().then(() => provider.cancel(s.id)).catch(() => {});
          notify();
        }
        throw error;
      }
    }
    async function finishInput() {
      const s = session;
      if (!s || s.state !== 'recording') return;
      s.state = 'transcribing';
      notify();
      try {
        const result = await s.provider.stop(s.id, s.input.signal);
        if (!current(s)) return;
        // A removed composer must never receive a delayed transcription.
        if (s.destination.connected && !s.destination.connected()) {
          await end();
          return;
        }
        if (result.text?.trim()) s.onTranscript(result.text);
        s.state = 'ready';
        notify();
        return s.epoch;
      } catch (error) {
        if (current(s)) {
          s.state = 'error';
          s.error = String(error);
          notify();
        }
      }
    }
    function interrupt() {
      if (!session) return;
      stopPlayback(session);
      notify();
    }
    function setMode(mode) {
      const s = session;
      if (!s) throw new Error('No voice session');
      if (!MODES.has(mode) || (mode !== 'silent' && !s.provider.capabilities.synthesis)) {
        throw new Error('This voice provider does not support that playback mode');
      }
      if (mode === s.mode) return;
      stopPlayback(s);
      s.mode = mode;
      notify();
    }
    function beginTurn(turnId, { summary = false } = {}) {
      const s = session;
      if (!s || !turnId) throw new Error('A voice session and turn ID are required');
      stopPlayback(s);
      s.turnId = turnId;
      s.summary = summary;
      s.seen.clear();
      s.error = s.mode === 'summary' && !summary ? 'This agent does not supply spoken summaries' : '';
      notify();
      return scope();
    }
    function scope() {
      return session && { epoch: session.epoch, turnId: session.turnId, generation: session.generation };
    }
    async function pump(s) {
      if (s.speaking || !s.queue.length) return;
      const generation = s.generation;
      s.speaking = true;
      notify();
      try {
        while (current(s) && s.generation === generation && s.queue.length) {
          const next = s.queue.shift();
          s.queueBytes -= bytes(next.text);
          await s.provider.speak(next.text, s.playback.signal);
        }
      } catch (error) {
        if (current(s) && s.generation === generation) {
          stopPlayback(s);
          s.error = `Speech stopped: ${error}. The written answer is unchanged.`;
          notify();
        }
      } finally {
        if (current(s) && s.generation === generation) {
          s.speaking = false;
          notify();
        }
      }
    }
    function offer(token, { id, kind, text }) {
      const s = session;
      if (!s || !token || !s.turnId || token.epoch !== s.epoch ||
          token.turnId !== s.turnId || token.generation !== s.generation) return 'stale';
      // Only explicitly classified assistant presentation enters the renderer.
      if (s.mode === 'silent') return 'silent';
      if (kind !== (s.mode === 'full' ? 'answer' : 'summary')) return 'filtered';
      if (kind === 'summary' && !s.summary) return 'unsupported';
      if (typeof id !== 'string' || !id || id.length > 128 || typeof text !== 'string' || !text.trim()) return 'invalid';
      if (kind === 'summary' && (bytes(text) > 1024 || text.trim().split(/\s+/).length > 80)) return 'invalid';
      if (s.seen.has(id)) return 'duplicate';
      if (s.seen.size >= MAX_SEGMENTS || s.queueBytes + bytes(text) > MAX_QUEUE_BYTES) {
        stopPlayback(s);
        s.error = 'Speech queue limit reached. Read the remaining answer in the conversation.';
        notify();
        return 'overflow';
      }
      s.seen.add(id);
      s.queue.push({ text });
      s.queueBytes += bytes(text);
      void pump(s);
      return 'queued';
    }
    return {
      begin, end, finishInput, interrupt, setMode, beginTurn, scope, offer, snapshot,
      subscribe(fn) { listeners.add(fn); fn(snapshot()); return () => listeners.delete(fn); },
    };
  }
  window.xnautCreateVoiceCoordinator = createCoordinator;
  window.xnautVoiceSession = createCoordinator();
})();
