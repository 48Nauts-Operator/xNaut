// The first voice slice uses existing local dictation. Do not advertise
// streaming, synthesis or provider choices until their adapters are available.
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
      overlay.innerHTML = '<strong data-voice-destination></strong><span data-voice-status role="status" aria-live="polite"></span><small>Local dictation · text goes to this composer · no automatic send</small><small data-voice-notice></small><div><button type="button" data-voice-finish>Finish dictation</button><button type="button" data-voice-end>Cancel</button></div>';
      overlay.querySelector('[data-voice-finish]').onclick = async () => {
        const epoch = await session.finishInput();
        if (epoch !== undefined) await session.end(epoch);
      };
      overlay.querySelector('[data-voice-end]').onclick = () => { void session.end(); };
      document.body.appendChild(overlay);
    }
    overlay.querySelector('[data-voice-destination]').textContent = state.destination;
    overlay.querySelector('[data-voice-notice]').textContent = state.notice || '';
    overlay.querySelector('[data-voice-notice]').hidden = !state.notice;
    overlay.querySelector('[data-voice-status]').textContent = state.error || ({
      starting: 'Starting microphone…', recording: 'Listening · up to 2 minutes',
      transcribing: 'Microphone off · transcribing…', ready: 'Text inserted',
    }[state.state] || state.state);
    overlay.querySelector('[data-voice-finish]').disabled = state.state !== 'recording';
  });
})();
