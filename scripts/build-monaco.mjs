#!/usr/bin/env node
// Monaco Editor 0.56.0 is Copyright Microsoft Corporation, MIT licensed.
// Build both a normal app-bundle worker and an inline fallback for WebKit,
// whose custom-protocol Worker loading is not reliable on installed apps.
import { build } from 'esbuild';
import { copyFile, mkdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const OUT = join(ROOT, 'src/js/vendor');
const LEGAL = '/*! Monaco Editor 0.56.0 | Copyright Microsoft Corporation | MIT License */';

await mkdir(OUT, { recursive: true });
const worker = await build({
  entryPoints: [join(ROOT, 'frontend/monaco-worker-entry.js')],
  bundle: true,
  minify: true,
  format: 'iife',
  platform: 'browser',
  target: 'safari15',
  write: false,
  banner: { js: LEGAL },
});
const workerSource = worker.outputFiles[0].text;

const inlineWorker = {
  name: 'xnaut-monaco-worker-source',
  setup(context) {
    context.onResolve({ filter: /^xnaut-monaco-worker-source$/ }, () => ({
      path: 'worker-source', namespace: 'xnaut-monaco',
    }));
    context.onLoad({ filter: /.*/, namespace: 'xnaut-monaco' }, () => ({
      contents: `export default ${JSON.stringify(workerSource)};`,
      loader: 'js',
    }));
  },
};

await build({
  entryPoints: [join(ROOT, 'frontend/monaco-editor-entry.js')],
  bundle: true,
  minify: true,
  format: 'iife',
  platform: 'browser',
  target: 'safari15',
  outfile: join(OUT, 'monaco.bundle.js'),
  loader: { '.ttf': 'dataurl' },
  plugins: [inlineWorker],
  banner: { js: LEGAL },
});

await writeFile(join(OUT, 'monaco.worker.js'), workerSource);
await copyFile(
  join(ROOT, 'node_modules/monaco-editor/LICENSE'),
  join(OUT, 'monaco.LICENSE.txt'),
);
