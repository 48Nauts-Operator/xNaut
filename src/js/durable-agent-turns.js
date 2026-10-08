// Native Agent Space execution outbox. Completed turns are projected into their
// original saved thread before acknowledgement; opening a different project or
// agent never redirects a recovered result. Execution belongs to the backend.
(function () {
  'use strict';
  const KEY = 'xnaut-agent-threads:v1';
  const active = new Set();
  const submitting = new Set();
  let reconciling;
  function report(error) {
    console.error('[durable-agent-turns]', error);
    let banner = document.getElementById('durable-turn-error');
    if (!banner) {
      banner = document.createElement('div');
      banner.id = 'durable-turn-error'; banner.setAttribute('role', 'alert');
      banner.style.cssText = 'position:fixed;bottom:55px;left:16px;right:16px;z-index:99999;background:#4b2727;color:white;padding:12px';
      document.body.appendChild(banner);
    }
    banner.textContent = `Agent recovery: ${error}. Saved execution results are retained.`;
  }
  function reconcile() {
    if (reconciling) return reconciling;
    reconciling = (async () => {
      const store = window.xnautConversationStorage;
      const invoke = window.__TAURI__?.core?.invoke;
      if (!store || !invoke) return;
      await store.ready();
      const turns = await invoke('durable_agent_turns');
      if (!Array.isArray(turns)) return;
      // Read after the await: an in-flight user edit must not be overwritten by
      // a snapshot taken before the native query completed.
      const all = JSON.parse(store.getItem(KEY) || '{}');
      const changed = new Set(), acknowledgements = [];
      const known = new Set(turns.map(turn => turn.request_id));
      // Cover a crash after saving the request but before native admission.
      for (const [handle, threads] of Object.entries(all)) {
        for (const thread of Array.isArray(threads) ? threads : []) {
          if (thread.archived_at) continue;
          for (const message of thread.messages || []) {
            const request = message.executionRequest, id = message.executionRequestId;
            if (!request || known.has(id) || active.has(id) || submitting.has(id)) continue;
            if (request.requestId !== id || request.handle !== handle || request.threadId !== thread.id) continue;
            submitting.add(id);
            invoke('agent_chat_turn', request).catch(report).finally(() => { submitting.delete(id); reconcile(); });
          }
        }
      }
      for (const turn of turns) {
        if (active.has(turn.request_id)) continue;
        const thread = all[turn.handle]?.find(item => item.id === turn.thread_id);
        const message = thread?.messages?.find(item => item.executionRequestId === turn.request_id);
        // Deleted/archived conversations are not recreated or silently resumed
        // into the currently visible thread. Their execution record stays kept.
        if (!message || thread.archived_at) continue;
        for (const value of turn.execution_receipts || []) {
          const receipt = value.receipt || value, launch = receipt.launch;
          if (!receipt.ok || !launch?.session_id) continue;
          if (!thread.messages.some(item => item.executionReceipt?.launch?.session_id === launch.session_id)) {
            thread.messages.push({ id: `launch-${launch.session_id}`, kind: 'action', label: 'Worker launched',
              detail: receipt.worktree_path, session_id: launch.session_id, executionReceipt: receipt, at: message.at });
            thread.session_id = launch.session_id;
            thread.workspace = receipt.worktree_path;
            changed.add(thread.id);
          }
        }
        if (!['completed', 'failed'].includes(turn.status)) {
          if (message.executionStatus !== 'recovering') {
            message.executionStatus = 'recovering';
            message.text = 'Recovering interrupted turn…';
            changed.add(thread.id);
          }
          continue;
        }
        message.text = turn.status === 'completed' ? (turn.result || 'No answer came back.') : `Could not answer: ${turn.error || 'Execution failed'}`;
        if (turn.status === 'completed' && message.text.startsWith('BUILD-REQUEST')) {
          message.build_task ||= message.executionRequest?.messages?.findLast(item => item.role === 'user')?.content || '';
          message.text = message.text.split('\n').slice(1).join('\n').trim() || 'That needs a coding session.';
        }
        message.executionStatus = turn.status;
        delete message.executionRequest;
        message.journalPending = false;
        const plan = turn.outcome?.swarm_plan;
        if (plan && !thread.messages.some(item => item.kind === 'swarm' && item.plan?.id === plan.id)) {
          thread.messages.push({ id: `swarm-${turn.request_id}`, kind: 'swarm', plan, at: message.at });
        }
        const plugin = turn.outcome?.needs_auth;
        if (plugin && !thread.messages.some(item => item.kind === 'auth' && JSON.stringify(item.plugin) === JSON.stringify(plugin))) {
          thread.messages.push({ id: `auth-${turn.request_id}`, kind: 'auth', plugin, at: message.at });
        }
        changed.add(thread.id);
        acknowledgements.push(turn.request_id);
      }
      if (changed.size) {
        store.setItem(KEY, JSON.stringify(all));
        await store.confirmSaved(KEY);
        window.dispatchEvent(new CustomEvent('xnaut:durable-turns-restored', { detail: { threads: [...changed] } }));
        window.dispatchEvent(new CustomEvent('xnaut:agent-threads-changed'));
      }
      for (const requestId of acknowledgements) await invoke('durable_agent_turn_ack', { requestId });
      document.getElementById('durable-turn-error')?.remove();
    })().catch(report).finally(() => { reconciling = null; });
    return reconciling;
  }
  window.xnautDurableAgentTurns = {
    track: id => active.add(id),
    finished: id => { active.delete(id); return reconcile(); },
    reconcile,
  };
  async function start() {
    if (!window.__TAURI__?.core?.invoke) return;
    await reconcile();
    window.setInterval(reconcile, 3000);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, { once: true });
  else start();
})();
