// Minimal zero-dependency static file server for the src/ frontend.
// Used by the Playwright tests and the demo to serve the app over http://.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../src/', import.meta.url));
const PORT = Number(process.env.PORT || 4173);

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.ttf': 'font/ttf',
  '.json': 'application/json',
};

const server = createServer(async (req, res) => {
  try {
    let path = decodeURIComponent((req.url || '/').split('?')[0]);
    if (path === '/') path = '/index.html';
    const filePath = join(ROOT, normalize(path).replace(/^(\.\.[/\\])+/, ''));
    if (!filePath.startsWith(ROOT)) { res.writeHead(403).end('Forbidden'); return; }
    const body = await readFile(filePath);
    res.writeHead(200, { 'Content-Type': TYPES[extname(filePath)] || 'application/octet-stream' });
    res.end(body);
  } catch (_) {
    res.writeHead(404, { 'Content-Type': 'text/plain' }).end('Not found');
  }
});

server.listen(PORT, () => console.log(`static-server: http://127.0.0.1:${PORT}`));
