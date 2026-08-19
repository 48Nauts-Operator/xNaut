// Say plainly when the browser is gone, because it keeps going.
//
// macOS purges ~/Library/Caches under storage pressure, and Playwright keeps
// its browsers there. On this machine the data volume sits at 96% full, so
// `com.apple.cache_delete` has taken chromium-headless-shell four times in one
// day. Every time, the suite fails with "Executable doesn't exist" and the
// mutation harness reports it as a BASELINE failure — which reads as a broken
// test rather than a missing binary, and sent one investigation down the wrong
// path already.

import { existsSync, readdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

export default function globalSetup() {
  const cache = process.env.PLAYWRIGHT_BROWSERS_PATH || join(homedir(), 'Library', 'Caches', 'ms-playwright');
  const present = existsSync(cache) && readdirSync(cache).some((entry) => entry.startsWith('chromium'));
  if (present) return;
  throw new Error(
    [
      '',
      'No Playwright browser is installed.',
      '',
      `  looked in: ${cache}`,
      '  fix:       npx playwright install chromium',
      '',
      'This is usually not something you broke. macOS purges ~/Library/Caches',
      'under storage pressure and Playwright keeps its browsers there, so a full',
      'disk quietly uninstalls them. Check free space if it keeps happening.',
      '',
    ].join('\n'),
  );
}
