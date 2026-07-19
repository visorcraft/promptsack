#!/usr/bin/env node
import { cpSync, mkdirSync, existsSync, writeFileSync, readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as esbuild from 'esbuild';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const ui = join(root, 'ui');
mkdirSync(ui, { recursive: true });

// Ensure TS is built
const appSrc = join(root, 'dist/renderer/app.js');
if (!existsSync(appSrc)) {
  console.error('[bundle-ui] dist/renderer/app.js missing — run tsc first');
  process.exit(1);
}

cpSync(join(root, 'src/renderer/styles.css'), join(ui, 'styles.css'));
cpSync(join(root, 'src/renderer/index.html'), join(ui, 'index.html'));
cpSync(join(root, 'src/renderer/bridge.js'), join(ui, 'bridge.js'));

await esbuild.build({
  entryPoints: [appSrc],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  outfile: join(ui, 'app.js'),
  logLevel: 'info',
});

// Ensure bridge is loaded before the app module
let html = readFileSync(join(ui, 'index.html'), 'utf8');
if (!html.includes('bridge.js')) {
  html = html.replace(
    '<script type="module" src="app.js"></script>',
    '<script src="bridge.js"></script>\n    <script type="module" src="app.js"></script>',
  );
  writeFileSync(join(ui, 'index.html'), html);
}

console.log('[bundle-ui] ui ready');
