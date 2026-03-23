// ABOUTME: Tauri API bridge for ES modules — exposes invoke/listen from the global Tauri object.
// ABOUTME: Needed because app.js declares invoke/listen as let-scoped, invisible to ES modules.

function getInvoke() {
  return window.__TAURI__?.core?.invoke;
}

function getListen() {
  return window.__TAURI__?.event?.listen;
}

/**
 * Invoke a Tauri command. Lazily resolves the Tauri API on first call.
 */
export async function invoke(cmd, args) {
  const fn = getInvoke();
  if (!fn) throw new Error('Tauri API not available');
  return fn(cmd, args);
}

/**
 * Listen to a Tauri event. Lazily resolves the Tauri API on first call.
 */
export async function listen(event, handler) {
  const fn = getListen();
  if (!fn) throw new Error('Tauri event API not available');
  return fn(event, handler);
}
