# Voice Preview — manual acceptance

Bundle: `src-tauri/target/debug/bundle/macos/xNAUT Voice Preview.app`.
Open it after recording and closing the earlier Voice Test app. The preview
retains the Voice Test bundle identifier to retain that app's saved conversations.
It shares the native xNaut settings. No API key is embedded in the bundle.

1. Open **⋯ → Settings → Voice**. Existing setup shows a saved-key indicator
   with an empty password field. New users enter their own key and save.
   **Test connection** opens a brief, potentially billable session without mic audio.
2. Open **+ → New Chat**, or select an Agent Space conversation. Choose
   **mic → STS**. Confirm hands-free turns, saved text and audible replies.
3. During a delegated request, confirm red **Thinking** matches the chat;
   after completion, active playback is green **Speaking**.
4. Choose **Mute microphone** in the voice pane. Wait for **Microphone muted**,
   then speak a distinct test phrase. It must not appear in captions or trigger
   an agent request. Replies may continue playing. The OS can still show mic use
   because the device remains allocated; outgoing mic audio is blocked.
5. Choose **Unmute microphone** and continue the same conversation. No muted
   audio should be replayed or transcribed. Previously sent audio may finish
   transcribing after the mute acknowledgement.
6. Choose **STT** and repeat mute/unmute. Unmuted words go into the draft;
   neither STT nor unmuting should send the draft automatically.
7. Reopen the saved Agent Space thread and resume; check context and no duplicated
   user/agent messages. End voice and confirm normal typed chat still works.

Automated checks cover UI/state transitions, credential persistence and mute
fencing. They do not replace this live microphone/speaker acceptance.
