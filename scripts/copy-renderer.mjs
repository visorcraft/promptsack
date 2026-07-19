#!/usr/bin/env node
/**
 * Copy static renderer assets next to compiled JS and ensure app.js is available.
 * tsc emits app.js from app.ts; HTML/CSS need a manual copy.
 */
import { cpSync, mkdirSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const src = join(root, 'src', 'renderer');
const dest = join(root, 'dist', 'renderer');

mkdirSync(dest, { recursive: true });

for (const file of ['index.html', 'styles.css']) {
  const from = join(src, file);
  if (existsSync(from)) {
    cpSync(from, join(dest, file));
  }
}

// Ensure app.js exists (tsc should have produced it)
const appJs = join(dest, 'app.js');
if (!existsSync(appJs)) {
  console.warn('[copy-renderer] dist/renderer/app.js missing — run tsc first');
} else {
  // Patch HTML if needed
  const htmlPath = join(dest, 'index.html');
  if (existsSync(htmlPath)) {
    let html = readFileSync(htmlPath, 'utf8');
    if (!html.includes('app.js')) {
      html = html.replace('</body>', '  <script type="module" src="app.js"></script>\n  </body>');
      writeFileSync(htmlPath, html);
    }
  }
}

console.log('[copy-renderer] renderer assets ready at dist/renderer');
