// XNAUT-416 V1: the public continuous voice conversation.
//
// One explicit start holds the whole conversation. There is no Finish & send
// and no Speak again here — the backend streams the microphone, streams the
// reply, and decides turn boundaries. That is the difference from the earlier
// push-to-talk prototype in voice-conversation.js, which stays for the private
// V2 local route.
//
// This file owns no audio and no transcript logic. The backend session
// (src-tauri/src/voice_live/) decides what happens; here we render it and run
// the dispatched turn through the surface's existing send path, so permissions,
// history and provider routing are the ones the app already resolved.
(() => {
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const listen = (...args) => window.__TAURI__.event.listen(...args);

  let active = null;

  function createOverlay() {
    const overlay = document.createElement('section');
    overlay.className = 'voice-live-overlay';
    overlay.setAttribute('aria-label', 'Voice conversation');
    overlay.innerHTML = '<strong data-live-destination></strong>'
      + '<span data-live-status role="status" aria-live="polite"></span>'
      + '<p data-live-caption aria-live="polite"></p>'
      + '<small data-live-policy></small>'
      + '<div><button type="button" data-live-end>End voice</button></div>';
    overlay.querySelector('[data-live-end]').onclick = () => {
      void teardown('Start a voice conversation');
    };
    document.body.appendChild(overlay);
    return overlay;
  }

  function render(state) {
    if (!active) return;
    const { overlay } = active;
    overlay.querySelector('[data-live-destination]').textContent = active.label;
    overlay.querySelector('[data-live-status]').textContent = state;
    overlay.querySelector('[data-live-policy]').textContent = active.restored
      ? 'Public voice · this conversation\'s earlier turns are in context'
      : 'Public voice · speech leaves this Mac; the selected agent runs the work';
    const caption = overlay.querySelector('[data-live-caption]');
    caption.textContent = active.caption;
    caption.hidden = !active.caption;
  }

  /// Captions are a running ribbon, not the conversation. They are replaced
  /// when the speaker changes so the panel never looks like a second history.
  function caption(role, text) {
    if (!active) return;
    if (active.captionRole !== role) {
      active.captionRole = role;
      active.caption = '';
    }
    active.caption = `${active.caption}${text}`.slice(-400);
  }

  async function teardown(reason) {
    const session = active;
    if (!session) return;
    active = null;
    session.button.setAttribute('aria-pressed', 'false');
    session.button.title = reason || 'Start a voice conversation';
    session.observer?.disconnect();
    try { (await session.unlisten)(); } catch (_) { /* already detached */ }
    session.overlay.remove();
    // Closing an already-closed session is not an error worth showing: the
    // backend ends it on socket loss too, and this path also runs from there.
    try { await invoke('voice_live_close', { sessionId: session.id }); } catch (_) { /* already ended */ }
  }

  async function handle(event, surface) {
    const payload = event.payload || {};
    // A session that has already been torn down must not paint or dispatch.
    if (!active || active.id !== surface.id) return;
    switch (payload.kind) {
      case 'ready':
        active.restored = !!payload.restored;
        render('Listening — just talk');
        break;
      case 'caption':
        caption(payload.role, payload.text || '');
        render(active.working ? 'Working…' : 'Listening — just talk');
        break;
      case 'commit':
        // The conversation record. Exactly one per turn per role; the backend
        // guarantees that, so this appends without de-duplicating again.
        surface.onCommit(payload.role, payload.text);
        break;
      case 'interrupted':
        active.caption = '';
        render('Stopped — go ahead');
        break;
      case 'dispatch':
        await runTurn(payload, surface);
        break;
      case 'ended':
        await teardown(payload.reason);
        break;
      default:
        // An unknown event from a newer backend must not break a live call.
        break;
    }
  }

  /// Runs one delegated turn on the selected xNAUT agent and returns its real
  /// outcome. A failure is reported as a failure; it is never dressed up as an
  /// answer, because the user will hear whatever comes back.
  async function runTurn(request, surface) {
    const session = active;
    session.working = true;
    render('Working…');
    let answer = '';
    let failed = false;
    try {
      answer = String(await surface.dispatch(request) || '').trim();
      if (!answer) {
        answer = 'The agent returned nothing for that turn.';
        failed = true;
      }
    } catch (error) {
      answer = String(error && error.message ? error.message : error);
      failed = true;
    }
    if (!active || active.id !== session.id) return;
    session.working = false;
    render('Listening — just talk');
    try {
      await invoke('voice_live_result', {
        sessionId: session.id,
        outcome: { turn: request.turn, epoch: request.epoch, answer, failed },
      });
    } catch (_) {
      // The session ended while the agent was working. The written answer is
      // already in the conversation; there is nothing left to speak into.
    }
  }

  /// `surface` supplies the conversation this voice session is pinned to:
  ///   label      — what the overlay calls it
  ///   connected  — false once the pane is gone
  ///   binding    — selected agent/project/provider/model/permission
  ///   history    — the saved conversation, oldest first, for context replay
  ///   onCommit   — persist one committed message
  ///   dispatch   — run one turn and resolve with the agent's answer
  function attach(hostButton, surface) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'voice-live-button';
    button.textContent = 'Voice';
    button.title = 'Start a voice conversation';
    button.setAttribute('aria-label', `Voice conversation with ${surface.label}`);
    button.setAttribute('aria-pressed', 'false');
    hostButton.insertAdjacentElement('afterend', button);

    // Offer the control only when a profile exists. A button that always fails
    // on click is worse than one that explains why it is disabled.
    invoke('voice_live_ready').then((ready) => {
      button.disabled = !ready;
      if (!ready) button.title = 'Public voice needs a profile in xnaut/voice-live.json';
    }).catch(() => { button.disabled = true; });

    button.onclick = async () => {
      if (active && active.button === button) { await teardown('Start a voice conversation'); return; }
      if (active) {
        button.title = 'End the current voice conversation first';
        return;
      }
      const id = crypto.randomUUID();
      surface.id = id;
      const overlay = createOverlay();
      const session = {
        id, button, overlay, label: surface.label,
        caption: '', captionRole: null, restored: false, working: false,
        unlisten: listen(`voice-live://${id}`, (event) => {
          handle(event, surface).catch((error) => console.error('[voice-live] event failed', error));
        }),
        observer: new MutationObserver(() => {
          if (!surface.connected()) void teardown('The conversation closed.');
        }),
      };
      active = session;
      session.observer.observe(document.body, { childList: true, subtree: true });
      button.setAttribute('aria-pressed', 'true');
      render('Connecting…');
      try {
        await session.unlisten;
        await invoke('voice_live_open', {
          sessionId: id,
          binding: surface.binding(),
          history: surface.history(),
        });
      } catch (error) {
        await teardown(String(error));
      }
    };

    return {
      end: () => (active && active.button === button ? teardown('Start a voice conversation') : Promise.resolve()),
      isActive: () => !!active && active.button === button,
      /// Typed input while the conversation is live. It goes into the same turn
      /// ledger as speech, so the assistant hears it and it is committed once,
      /// rather than opening a second parallel conversation in the composer.
      send: (text) => invoke('voice_live_text', { sessionId: active.id, text }),
    };
  }

  window.xnautAttachLiveVoice = attach;
})();
