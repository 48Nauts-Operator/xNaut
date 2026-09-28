// Credentials travel only to the native profile writer; never localStorage.
(() => {
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  window.xnautRenderVoiceSettings = async (host) => {
    if (!host) return;
    host.innerHTML = `<h3>Voice</h3>
      <div class="settings-group">
        <h4>Public voice</h4>
        <p>Use your own OpenAI API key for streaming speech. STS gives spoken replies; STT fills your draft without sending it.</p>
        <p>Both modes send microphone audio to the voice provider. Your selected agent handles requests using its own model settings.</p>
        <form data-voice-form>
          <div class="settings-row"><label for="voice-api-key">API key</label>
            <input id="voice-api-key" type="password" autocomplete="off" spellcheck="false" placeholder="Enter your API key" maxlength="512"></div>
          <div class="settings-row"><label for="voice-model">Voice model</label>
            <input id="voice-model" type="text" value="gpt-live-1" required maxlength="128" autocomplete="off" spellcheck="false"></div>
          <p data-voice-credential>Loading saved configuration…</p>
          <p>The key is stored in a private local profile on this device and is never displayed again. Leave it blank to keep an existing key.</p>
          <div class="settings-row">
            <button type="submit" class="btn btn-primary" data-voice-save disabled>Save voice settings</button>
            <button type="button" class="btn-test" data-voice-test disabled>Test connection</button>
          </div>
          <p>Test uses the saved settings to open a brief provider session. It sends no microphone audio and may incur API usage.</p>
          <p data-voice-status role="status" aria-live="polite"></p>
        </form>
      </div>`;
    const form = host.querySelector('[data-voice-form]');
    const key = host.querySelector('#voice-api-key');
    const model = host.querySelector('#voice-model');
    const status = host.querySelector('[data-voice-status]');
    const credential = host.querySelector('[data-voice-credential]');
    const save = host.querySelector('[data-voice-save]');
    const test = host.querySelector('[data-voice-test]');
    let configured = false;
    let busy = false;
    let dirty = false;
    const controls = () => {
      save.disabled = busy;
      test.disabled = busy || dirty || !configured;
      key.disabled = model.disabled = busy;
    };
    const apply = (saved) => {
      configured = !!saved.configured;
      model.value = saved.model || 'gpt-live-1';
      key.value = '';
      key.placeholder = configured ? 'Leave blank to keep your saved key' : 'Enter your API key';
      credential.textContent = configured ? 'API key saved on this device.' : 'No API key saved yet.';
      dirty = false;
    };
    try { apply(await invoke('voice_live_settings_get')); }
    catch (error) {
      credential.textContent = 'Saved configuration could not be loaded. Enter a key to replace it.';
      status.textContent = String(error?.message || error);
    }
    controls();
    form.addEventListener('input', () => { dirty = true; controls(); });
    form.onsubmit = async (event) => {
      event.preventDefault();
      if (busy) return;
      if (!configured && !key.value.trim()) { status.textContent = 'Enter your API key first.'; key.focus(); return; }
      busy = true; controls(); status.textContent = 'Saving…';
      try {
        const saved = await invoke('voice_live_settings_save', { apiKey: key.value.trim() || null, model: model.value.trim() });
        apply(saved);
        status.textContent = 'Saved. Start STS or STT from the microphone in Chat or Agent Space. Changes apply to the next voice session.';
        document.dispatchEvent(new CustomEvent('xnaut:voice-settings-changed'));
      } catch (error) { status.textContent = `Could not save: ${String(error?.message || error)}`; }
      finally { busy = false; controls(); }
    };
    test.onclick = async () => {
      if (busy || dirty || !configured) return;
      busy = true; controls(); status.textContent = 'Testing saved voice connection…';
      try {
        await invoke('voice_live_settings_test');
        status.textContent = 'Connected. The voice service accepted your key and model.';
      } catch (error) { status.textContent = `Connection failed: ${String(error?.message || error)}`; }
      finally { busy = false; controls(); }
    };
  };
})();
