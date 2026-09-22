// Monaco Editor 0.56.0 is Copyright Microsoft Corporation, MIT licensed.
// This entry bundles Monaco; it does not port application code from it.
import * as monaco from 'monaco-editor/editor/editor.api.js';
import 'monaco-editor/editor/browser/widget/codeEditor/codeEditorWidget.js';
import 'monaco-editor/editor/browser/widget/diffEditor/diffEditor.contribution.js';
import 'monaco-editor/editor/contrib/bracketMatching/browser/bracketMatching.js';
import 'monaco-editor/editor/contrib/clipboard/browser/clipboard.js';
import 'monaco-editor/editor/contrib/comment/browser/comment.js';
import 'monaco-editor/editor/contrib/find/browser/findController.js';
import 'monaco-editor/editor/contrib/hover/browser/hoverContribution.js';
import 'monaco-editor/editor/contrib/linesOperations/browser/linesOperations.js';
import 'monaco-editor/editor/contrib/suggest/browser/suggestController.js';
import '../node_modules/monaco-editor/esm/vs/editor/standalone/browser/standalone-tokens.css';
import '../node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon.css';
import 'monaco-editor/languages/register.all.js';
import workerSource from 'xnaut-monaco-worker-source';

window.XnautMonacoBundle = { monaco, workerSource };
