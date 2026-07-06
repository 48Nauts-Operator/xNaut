#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = new URL('..', import.meta.url).pathname;
const read = (path) => readFileSync(join(root, path), 'utf8');
const failures = [];

const app = read('src/js/app.js');
const chat = read('src/js/chat-panel.js');
const rightPane = read('src/js/right-pane.js');
const vaultPane = read('src/js/vault-pane.js');
const markdownRender = read('src/js/markdown-render.js');
const rustChat = read('src-tauri/src/chat.rs');
const rustVault = read('src-tauri/src/vault.rs');
const rightPaneHostTemplate = (rightPane.match(/hostElement\.innerHTML = `([\s\S]*?)`;/) || [])[1] || '';
const librarianViewSection = rightPane.slice(Math.max(0, rightPane.indexOf('function createLibrarianView')));
const librarianPaneTemplate = (librarianViewSection.match(/container\.innerHTML = `([\s\S]*?)`;/) || [])[1] || '';
const chatActionWhitelist = (chat.match(/const known = \[([\s\S]*?)\];/) || [])[1] || '';

function expect(name, condition) {
  if (!condition) failures.push(name);
}

expect(
  'AI Settings save syncs the Rust chat settings store',
  /settings_set/.test(app) && /xnautSyncChatSettingsFromAiSettings/.test(app),
);

expect(
  'LM Studio Settings endpoint is normalized for OpenAI-compatible chat_send',
  /provider\s*===\s*'lmstudio'[\s\S]*\/v1/.test(app),
);

expect(
  'Librarian parser accepts Gemma native tool-call syntax',
  /parseGemmaToolCalls/.test(chat) && /<\\\|tool_call>call:/.test(chat),
);

expect(
  'Librarian parser accepts tool-key vault actions from local models',
  /j\.tool/.test(chat) && /j\.action\s*=\s*j\.tool/.test(chat),
);

expect(
  'Librarian parser accepts vault_write actions that the executor supports',
  /'vault_write'/.test(chatActionWhitelist) && /action\.action === 'vault_write'/.test(chat),
);

expect(
  'Polite can-you vault requests still trigger note actions',
  /can\|could\|should\|would/.test(chat)
    && !/can\|could\|should\|would\)\\s\+\(we\|you\|i\)/.test(chat)
    && /we\|i/.test(chat),
);

expect(
  'Librarian parser tolerates local-model vault_create JSON with long markdown content',
  /parseLooseVaultAction/.test(chat)
    && /contentMatch/.test(chat)
    && /replace\(\/\\\\n\/g/.test(chat),
);

expect(
  'Readable Markdown attachments can be imported into the Vault without model-generated JSON',
  /shouldImportAttachmentsToVault/.test(chat)
    && /importAttachmentsToVault/.test(chat)
    && /fullContent/.test(chat)
    && /vault_note_create/.test(chat)
    && /vault_note_write/.test(chat)
    && /openNote/.test(chat)
    && /_inbox/.test(chat),
);

expect(
  'Explicit user-supplied vault action JSON executes without model echo',
  /userVaultActions/.test(chat)
    && /detectScaffoldActions\(text\)/.test(chat)
    && /runVaultTools\(entry, row, userVaultActions/.test(chat)
    && /entry\.vaultTools\.entry\.refresh/.test(chat),
);

expect(
  'Vault tool output renders as user-facing action status, not raw tool markup',
  /chatp-tool-summary/.test(chat) && /chatp-tool-results/.test(chat),
);

expect(
  'Vault tool statuses use note wording instead of exposing vault_create',
  /vaultActionLabel/.test(chat) && /create note/.test(chat) && !/vault_create failed/.test(chat),
);

expect(
  'Direct Librarian mutation requests force tool-only action replies',
  /latestUserNeedsVaultAction/.test(chat) && /Reply with ONLY the required vault tool JSON/.test(chat),
);

expect(
  'Direct Librarian mutation requests suppress streamed prose until action outcome',
  /expectingVaultAction/.test(chat) && /Preparing note action/.test(chat),
);

expect(
  'Librarian repair prompt remains a user query for LM Studio Qwen templates',
  /previous response was prose/.test(chat) && /role:\s*'user'/.test(chat),
);

expect(
  'Direct Librarian mutation requests do not fall back to plan document updates',
  /No note action was produced/.test(chat) && /if \(needsVaultAction\)/.test(chat),
);

expect(
  'Vault tool-result followup ends as a user query for LM Studio Qwen templates',
  /lastMessageIsVaultToolResult/.test(chat) && /Continue from the vault tool results above/.test(chat),
);

expect(
  'Read/search tool results still force a follow-up vault mutation when the user asked to save a note',
  /latestUserWantsVaultMutation/.test(chat)
    && /lastVaultToolResultsHaveMutation/.test(chat)
    && /pendingVaultMutation/.test(chat)
    && /original user request still requires/.test(chat),
);

expect(
  'Qwen chat requests disable hidden reasoning so actions stream as content',
  /should_disable_reasoning_for_chat/.test(rustChat) && /reasoning_effort/.test(rustChat),
);

expect(
  'Qwen chat requests always end with a user query for LM Studio templates',
  /normalize_messages_for_model_template/.test(rustChat) && /messages\.last\(\)/.test(rustChat),
);

expect(
  'Vault write commands emit change events so the visible tree refreshes',
  /fn emit_vault_changed/.test(rustVault) && /vault_note_write[\s\S]*emit_vault_changed/.test(rustVault),
);

expect(
  'Vault document preview and editor have their own scroll containers',
  /plan-doc-view/.test(vaultPane) && /overflow:auto/.test(vaultPane) && /plan-doc-edit/.test(vaultPane),
);

expect(
  'Vault document preview hides YAML frontmatter without changing edit content',
  /stripPreviewFrontmatter/.test(vaultPane)
    && /renderInto\(view,\s*stripPreviewFrontmatter\(ta\.value\)/.test(vaultPane),
);

expect(
  'Markdown preview applies shared document styling with spaced headings and polished tables',
  /classList\.add\('xnaut-md'\)/.test(markdownRender)
    && /\.xnaut-md h2 \{[^}]*margin:34px 0 12px/.test(markdownRender)
    && /\.xnaut-md h3 \{[^}]*margin:28px 0 10px/.test(markdownRender)
    && /\.xnaut-md table \{[^}]*border-spacing:0/.test(markdownRender)
    && /\.xnaut-md tbody tr:nth-child\(odd\)/.test(markdownRender)
    && /\.xnaut-md tbody tr:hover/.test(markdownRender),
);

expect(
  'Vault create panel can create a note from a selected template',
  /vp-template-select/.test(vaultPane)
    && /vp-create-template/.test(vaultPane)
    && /templateNotes/.test(vaultPane)
    && /vault_note_read/.test(vaultPane)
    && /applyTemplateVars/.test(vaultPane),
);

expect(
  'Librarian conversation history is selected from the far-right rail and new conversation is an in-pane action',
  /rpane-librarian-history/.test(rightPane)
    && /rpane-librarian-new/.test(rightPane)
    && /rpane-librarian-panel/.test(rightPane)
    && /archiveCurrentLibrarianConversation/.test(rightPane)
    && /xnautSetChatHistory/.test(chat)
    && /xnautClearChatHistory/.test(chat)
    && /rpane-librarian-history/.test(rightPaneHostTemplate)
    && !/rpane-librarian-new/.test(rightPaneHostTemplate)
    && /rpane-librarian-head/.test(librarianPaneTemplate)
    && /rpane-librarian-new/.test(librarianPaneTemplate)
    && !/vp-history-toggle/.test(vaultPane),
);

expect(
  'Opening Vault selects the far-right Librarian conversation pane',
  /showLibrarianConversations/.test(rightPane)
    && /xnautRightPaneShowLibrarianConversations/.test(rightPane)
    && /showLibrarianConversationsPane/.test(vaultPane)
    && /vault_open[\s\S]*refresh\(\)[\s\S]*showLibrarianConversationsPane/.test(vaultPane),
);

if (failures.length) {
  console.error('Chat contract failures:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log('Chat contracts passed');
