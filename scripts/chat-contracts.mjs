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
const agentsPanel = read('src/js/agents-panel.js');
const indexHtml = read('src/index.html');
const glue = read('src/js/tasks-mode-glue.js');
const markdownRender = read('src/js/markdown-render.js');
const markdownPane = read('src/js/markdown-pane.js');
const planPane = read('src/js/plan-pane.js');
const rustChat = read('src-tauri/src/chat.rs');
const rustVault = read('src-tauri/src/vault.rs');
const rustAgentProfiles = read('src-tauri/src/agent_profiles.rs');
const rightPaneHostTemplate = (rightPane.match(/hostElement\.innerHTML = `([\s\S]*?)`;/) || [])[1] || '';
const librarianViewSection = rightPane.slice(Math.max(0, rightPane.indexOf('function createLibrarianView')));
const librarianPaneTemplate = (librarianViewSection.match(/container\.innerHTML = `([\s\S]*?)`;/) || [])[1] || '';
const chatActionWhitelist = (chat.match(/const known = \[([\s\S]*?)\];/) || [])[1] || '';
const runAgentFatherSection = agentsPanel.slice(
  Math.max(0, agentsPanel.indexOf('async function runAgentFather')),
  Math.max(0, agentsPanel.indexOf('function renderList')),
);

function expect(name, condition) {
  if (!condition) failures.push(name);
}

expect(
  'AI Settings save syncs the Rust chat settings store',
  /settings_set/.test(app) && /xnautSyncChatSettingsFromAiSettings/.test(app),
);

expect(
  'Agent profile runs can override the base model without changing global chat settings',
  /pub async fn chat_send_model/.test(rustChat)
    && /model_override/.test(rustChat)
    && /chat::chat_send_model/.test(read('src-tauri/src/main.rs'))
    && /"chat_send_model"/.test(read('src-tauri/permissions/default.toml')),
);

expect(
  'AgentFather profile commands are registered in Rust',
  /mod agent_profiles;/.test(read('src-tauri/src/main.rs'))
    && /agent_profiles::agent_profiles_list/.test(read('src-tauri/src/main.rs'))
    && /agent_profiles::agent_profile_save/.test(read('src-tauri/src/main.rs')),
);

expect(
  'Agents panel is wired into menu and panel tab system',
  /data-action="agents">Agents/.test(indexHtml)
    && /js\/agents-panel\.js/.test(indexHtml)
    && /xnautAttachAgentsTab/.test(glue)
    && /action === 'agents'/.test(app),
);

expect(
  'Agents panel exposes AgentFather and guardrail presets',
  /AgentFather/.test(agentsPanel)
    && /ACCESS_PRESETS/.test(agentsPanel)
    && /Full Project Access/.test(agentsPanel)
    && /xnautAgentAccessPresets\s*=\s*ACCESS_PRESETS/.test(agentsPanel)
    && /xnautOpenAgentFather/.test(agentsPanel)
    && /return\s*\{[^}]*kind:\s*'agents'[^}]*pane/.test(agentsPanel),
);

expect(
  'Agents panel implements Agent Library editor MVP',
  /agent_profiles_seed/.test(agentsPanel)
    && /agent_profiles_list/.test(agentsPanel)
    && /agent_profile_read/.test(agentsPanel)
    && /agent_profile_save/.test(agentsPanel)
    && /agent_profile_delete/.test(agentsPanel)
    && /Built-in/.test(agentsPanel)
    && /Custom/.test(agentsPanel)
    && /data-section="persona"/.test(agentsPanel)
    && /data-section="access"/.test(agentsPanel)
    && /data-section="markdown"/.test(agentsPanel)
    && /Full Project Access/.test(agentsPanel)
    && /kind:\s*'agents'/.test(agentsPanel)
    && /pane/.test(agentsPanel),
);

expect(
  'Agents panel separates AgentFather creation from profile editing and normal agent overview',
  /agent-tabs/.test(agentsPanel)
    && /Create Agents/.test(agentsPanel)
    && /Edit AgentFather/.test(agentsPanel)
    && /Overview/.test(agentsPanel)
    && /Edit Agent/.test(agentsPanel)
    && /renderOverview/.test(agentsPanel)
    && /isAgentFatherProfile/.test(agentsPanel)
    && /state\.activeTab === 'create'/.test(agentsPanel)
    && /state\.activeTab === 'overview'/.test(agentsPanel),
);

expect(
  'Agents panel stores runtime base model and default agent selection',
  /runtime:\s*\{\s*provider:\s*'global'/.test(agentsPanel)
    && /Runtime \/ Base Model/.test(agentsPanel)
    && /RUNTIME_PROVIDERS/.test(agentsPanel)
    && /DEFAULT_AGENT_KEY/.test(agentsPanel)
    && /agent-default-selector/.test(agentsPanel)
    && /renderDefaultAgentSelector/.test(agentsPanel)
    && /bar\.appendChild\(renderDefaultAgentSelector\(\)\)/.test(agentsPanel)
    && /setDefaultAgentRel/.test(agentsPanel)
    && /AgentRuntime/.test(rustAgentProfiles)
    && /runtime:\\n/.test(rustAgentProfiles),
);

expect(
  'Agents can be run from their overview through the existing chat backend',
  /Run Agent/.test(agentsPanel)
    && /runAgentProfile/.test(agentsPanel)
    && /profileSystemPrompt/.test(agentsPanel)
    && /const command = assignedModel \? 'chat_send_model' : 'chat_send'/.test(agentsPanel)
    && /invoke\(command,\s*payload\)/.test(agentsPanel)
    && /chat_send_model/.test(agentsPanel)
    && /assignedModel/.test(agentsPanel)
    && /Run with Global Model/.test(agentsPanel),
);

expect(
  'Agents panel keeps new profile defaults saveable and conservative',
  /Can Draft Docs/.test(agentsPanel)
    && /id:\s*'draft-docs'/.test(agentsPanel)
    && /status:\s*'enabled'/.test(agentsPanel)
    && /createStatusSelect/.test(agentsPanel)
    && /statusSelect\.value/.test(agentsPanel)
    && !/status:\s*statusInput\.value\.trim\(\)\s*\|\|\s*'draft'/.test(agentsPanel),
);

expect(
  'Agents panel makes Full Project Access visible and requires save acknowledgement',
  /Full Project Access can read\/write project docs, create handoffs, launch coding agents, inspect or edit repo files, and run tests\. Secrets remain denied\. Destructive actions still require confirmation\./.test(agentsPanel)
    && /fullProjectAccessAcknowledged/.test(agentsPanel)
    && /Confirm Full Project Access before saving this profile\./.test(agentsPanel)
    && /agent-full-access-ack/.test(agentsPanel),
);

expect(
  'Agents panel treats backend Builder-shaped access as requiring Full Project Access acknowledgement',
  /privilegedAccessRequiresAcknowledgement/.test(agentsPanel)
    && /source_code/.test(agentsPanel)
    && /assigned_files/.test(agentsPanel)
    && /repo/.test(agentsPanel)
    && /edit_source/.test(agentsPanel)
    && /access_preset\("builder"\)[\s\S]*source_code[\s\S]*assigned_files[\s\S]*secrets/.test(rustAgentProfiles),
);

expect(
  'AgentFather supports BotFather-style profile commands',
  /\/newagent/.test(agentsPanel)
    && /\/cloneagent/.test(agentsPanel)
    && /\/testagent/.test(agentsPanel)
    && /\/listagents/.test(agentsPanel)
    && /parseAgentFatherCommand/.test(agentsPanel)
    && /createProfileFromSeed/.test(agentsPanel),
);

expect(
  'AgentFather stays deterministic and avoids LLM dispatch',
  /deterministicAgentFather/.test(agentsPanel)
    && /agent_profile_test/.test(agentsPanel)
    && /includes\('create'\)[\s\S]*includes\('agent'\)/.test(agentsPanel)
    && !/chat_send/.test(runAgentFatherSection)
    && !/streamChat/.test(runAgentFatherSection),
);

expect(
  'AgentFather preserves live editor drafts while handling commands',
  /collectEditorSnapshot/.test(agentsPanel)
    && /appendTranscriptLine/.test(agentsPanel)
    && /flushTranscript/.test(agentsPanel)
    && /selectedPersisted/.test(agentsPanel)
    && /state\.selectedRel\s*&&\s*state\.selectedRel\s*===\s*selected\.rel/.test(agentsPanel),
);

expect(
  'AgentFather preserves edits made while dry-run testing is pending',
  /finally\s*\{[\s\S]*const refreshedEditorSnapshot = collectEditorSnapshot\(\)/.test(agentsPanel)
    && /const refreshedEditorSnapshot = collectEditorSnapshot\(\)/.test(agentsPanel)
    && /if \(refreshedEditorSnapshot\) state\.selected = refreshedEditorSnapshot/.test(agentsPanel)
    && /flushTranscript\(\);[\s\S]*\}\s*\}\s*\n\s*function renderList/.test(agentsPanel),
);

expect(
  'Agents panel ignores stale async profile loads and selections',
  /selectRequestId/.test(agentsPanel)
    && /loadRequestId/.test(agentsPanel)
    && /userInteracted/.test(agentsPanel)
    && /requestId\s*!==\s*state\.selectRequestId/.test(agentsPanel)
    && /requestId\s*!==\s*state\.loadRequestId/.test(agentsPanel)
    && /rel\s*!==\s*state\.selectedRel/.test(agentsPanel),
);

expect(
  'Agents panel tabs close without terminal cleanup',
  /terminal\s*&&\s*terminal\.kind\s*===\s*'agents'[\s\S]*continue;[\s\S]*close_terminal/.test(app),
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
  'Vault refresh rescans disk so externally-created notes appear without restart',
  /vault_tree[\s\S]*let root = idx\.root\.clone\(\);[\s\S]*\*idx = VaultIndex::build\(root\)/.test(rustVault)
    && /refreshExternalChanges/.test(vaultPane)
    && /visibilitychange/.test(vaultPane)
    && /window\.addEventListener\('focus'/.test(vaultPane),
);

expect(
  'Vault refresh control is an icon button, not a letter button',
  /vp-refresh" title="Refresh Vault" aria-label="Refresh Vault"[\s\S]*<svg viewBox="0 0 16 16"/.test(vaultPane)
    && !/vp-refresh" title="Refresh">R<\/button>/.test(vaultPane),
);

expect(
  'Vault document preview and editor have their own scroll containers',
  /plan-doc-view/.test(vaultPane) && /overflow:auto/.test(vaultPane) && /plan-doc-edit/.test(vaultPane),
);

expect(
  'Markdown Preview/Edit toggles render as icon pills across Vault, Plan, and Markdown panes',
  /MODE_TOGGLE_HTML/.test(vaultPane)
    && /MODE_TOGGLE_HTML/.test(planPane)
    && /MODE_TOGGLE_HTML/.test(markdownPane)
    && /aria-label="Preview"/.test(vaultPane)
    && /aria-label="Edit"/.test(vaultPane)
    && /viewBox="0 0 16 16"[\s\S]*<circle cx="8" cy="8" r="2"/.test(vaultPane)
    && /<path d="M3 13l1-3 6\.8-6\.8/.test(vaultPane)
    && /\.plan-doc-toggle button \{[^}]*width:28px/.test(vaultPane)
    && /\.md-toggle button \{[^}]*width:28px/.test(markdownPane)
    && !/>Preview<\/button>/.test(vaultPane)
    && !/>Edit<\/button>/.test(vaultPane),
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

expect(
  'Vault and Librarian can hand off agent setup requests to AgentFather without chat_send',
  /xnautOpenAgentFather/.test(vaultPane)
    && /openAgentFatherFromVault/.test(vaultPane)
    && /source:\s*'vault'/.test(vaultPane)
    && /vault:\s*seed\s*&&\s*seed\.vault/.test(vaultPane)
    && /rel:\s*seed\s*&&\s*seed\.rel/.test(vaultPane)
    && /responsibility:\s*seed\s*&&\s*seed\.responsibility/.test(vaultPane)
    && /entry\.openAgentFather\s*=\s*openAgentFatherFromVault/.test(vaultPane)
    && /maybeOpenAgentFather/.test(chat)
    && /agentFatherSeed/.test(chat)
    && /agentFatherDocumentIntent/.test(chat)
    && /agent\[-\\s\]\+profile/.test(chat)
    && /note\|file\|document\|doc\|template/.test(chat)
    && /\bvault\b/.test(chat)
    && /create\|make\|setup\|set\\s\+up\|new\|save\|import\|copy\|add\|write\)[\s\S]*\\bvault\\b/.test(chat)
    && /\\bvault\\b[\s\S]*called\|named\|about\|for/.test(chat)
    && /create\|make\|setup\)[\s\S]*\(\?:a\|an\|the\)[\s\S]*agents/.test(chat)
    && /set\\s\+up[\s\S]*\(\?:a\|an\|the\)[\s\S]*agents/.test(chat)
    && /if \(agentFatherDocumentIntent\.test\(s\)\) return false;[\s\S]*explicitProfileIntent/.test(chat)
    && /entry\.history\.push\(\{\s*role:\s*'assistant'[\s\S]*content:\s*msg/.test(chat)
    && /saveChatHistory\(entry\);[\s\S]*appendMessage\(entry,\s*'assistant',\s*msg\)/.test(chat)
    && /async function sendMessage\(entry\)[\s\S]*?const userVaultActions[\s\S]*?const shouldImportToVault[\s\S]*?if \(!userVaultActions\.length && !shouldImportToVault && maybeOpenAgentFather\(entry,\s*text\)\) \{[\s\S]*?return;[\s\S]*?\}[\s\S]*?const requestId = newRequestId\(\)/.test(chat)
    && /async function complete\(entry,\s*row\)[\s\S]*?chatCommand[\s\S]*?invoke\(chatCommand/.test(chat),
);

expect(
  'Forge items open in-app and can launch an assigned RCA agent in the resizable right pane',
  /forge_get_issue/.test(read('src/js/tasks-panel.js'))
    && /taskp-detail/.test(read('src/js/tasks-panel.js'))
    && /Analyze with Agent/.test(read('src/js/tasks-panel.js'))
    && /xnautRightPaneOpenChat/.test(read('src/js/tasks-panel.js'))
    && /learningContext/.test(read('src/js/tasks-panel.js'))
    && /\{ key: 'chat', title: 'Chat' \}/.test(rightPane)
    && /rpane-resize/.test(rightPane)
    && /xnaut-right-pane-width/.test(rightPane)
    && /xnautRightPaneOpenChat/.test(rightPane)
    && /opts\.autoSend/.test(chat)
    && /modelOverride/.test(chat),
);

expect(
  'Verified ticket reviews feed Engram and run through a daily consolidation loop',
  /engram_store_learning/.test(chat)
    && /Add verified learning to Engram/.test(chat)
    && /engram::engram_store_learning/.test(read('src-tauri/src/main.rs'))
    && /engram::engram_run_learning_loop/.test(read('src-tauri/src/main.rs'))
    && /spawn_daily_learning_task/.test(read('src-tauri/src/main.rs'))
    && /"\/memories"/.test(read('src-tauri/src/engram.rs'))
    && /"\/consolidation\/run"/.test(read('src-tauri/src/engram.rs'))
    && /confirmed_by_user/.test(read('src-tauri/src/engram.rs')),
);

if (failures.length) {
  console.error('Chat contract failures:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log('Chat contracts passed');
