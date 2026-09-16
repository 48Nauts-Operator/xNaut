// Shared dictation/conversation controls. Unsupported providers and agent
// summary transports remain unavailable rather than silently falling back.
(() => {
  const session = window.xnautVoiceSession;
  let overlay;
  session.subscribe((state) => {
    if (state.state === 'idle') {
      overlay?.remove();
      overlay = null;
      return;
    }
    if (!overlay) {
      overlay = document.createElement('section');
      overlay.className = 'voice-overlay';
      overlay.setAttribute('aria-label', 'Voice input');
      overlay.innerHTML = '<strong data-voice-destination></strong><span data-voice-status role="status" aria-live="polite"></span><small data-voice-policy></small><small data-voice-notice></small><label data-voice-playback>Speak <select aria-label="Spoken output"><option value="full">Full response prose</option><option value="silent">Silent</option></select></label><div><button type="button" data-voice-record>Speak again</button><button type="button" data-voice-finish>Finish dictation</button><button type="button" data-voice-interrupt>Stop speaking</button><button type="button" data-voice-end>Cancel</button></div>';
      overlay.querySelector('[data-voice-finish]').onclick = async () => {
        const persistent = session.snapshot().persistent;
        const epoch = await session.finishInput();
        if (epoch !== undefined && !persistent) await session.end(epoch);
      };
      overlay.querySelector('[data-voice-record]').onclick = () => { void session.recordAgain(); };
      overlay.querySelector('[data-voice-interrupt]').onclick = () => session.interrupt();
      overlay.querySelector('select').onchange = (event) => session.setMode(event.target.value);
      overlay.querySelector('[data-voice-end]').onclick = () => { void session.end(); };
      document.body.appendChild(overlay);
    }
    overlay.querySelector('[data-voice-destination]').textContent = state.destination;
    overlay.querySelector('[data-voice-policy]').textContent = state.persistent
      ? 'Local conversation · Finish & send submits your words · code stays on screen'
      : 'Local dictation · text goes to this composer · no automatic send';
    overlay.querySelector('[data-voice-playback]').hidden = !state.persistent;
    overlay.querySelector('select').value = state.mode || 'silent';
    overlay.querySelector('[data-voice-record]').hidden = !state.persistent;
    overlay.querySelector('[data-voice-record]').disabled = state.state !== 'ready' || state.turnBusy;
    overlay.querySelector('[data-voice-interrupt]').hidden = !state.persistent;
    overlay.querySelector('[data-voice-interrupt]').disabled = !state.speaking;
    overlay.querySelector('[data-voice-end]').textContent = state.persistent ? 'End voice' : 'Cancel';
    overlay.querySelector('[data-voice-finish]').textContent = state.persistent ? 'Finish & send' : 'Finish dictation';
    overlay.querySelector('[data-voice-notice]').textContent = state.notice || '';
    overlay.querySelector('[data-voice-notice]').hidden = !state.notice;
    overlay.querySelector('[data-voice-status]').textContent = state.error || (state.speaking ? 'Speaking…' : state.turnBusy ? 'Waiting for agent…' : ({
      starting: 'Starting microphone…', recording: 'Listening · up to 2 minutes',
      transcribing: 'Microphone off · transcribing…', ready: state.persistent ? 'Ready · microphone off' : 'Text inserted',
    }[state.state] || state.state));
    overlay.querySelector('[data-voice-finish]').disabled = state.state !== 'recording';
  });
})();
