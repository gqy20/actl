import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
const files = readdirSync(new URL('../dist/assets/', import.meta.url));
const readAsset = name => readFileSync(new URL(`../dist/assets/${name}`, import.meta.url), 'utf8');
const jsFiles = files.filter(name => name.endsWith('.js') && !name.startsWith('mocks-'));
if (jsFiles.length !== 1) throw new Error('Expected one JS entry');
// Production is host-only; the mock branch must not load a separate chunk.
const js = readAsset(jsFiles[0]).replace(/<\/script/gi, '<\\/script');
const css = files.filter(name => name.endsWith('.css')).map(readAsset).join('\n');
let html = readFileSync(new URL('../dist/index.html', import.meta.url), 'utf8')
  .replace(/<script[^>]*src="[^"]*"[^>]*><\/script>/, () => `<script type="module">${js}</script>`)
  .replace(/<link[^>]*rel="stylesheet"[^>]*>/, () => `<style>${css}</style>`);
const hash = createHash('sha256').update(js).digest('base64');
html = html.replace('<head>', `<head><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'sha256-${hash}'; style-src 'unsafe-inline'; img-src 'none'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'">`);
const output = new URL('../../../crates/actl-uia/assets/signal-details.html', import.meta.url);
if (process.argv.includes('--check')) {
  if (readFileSync(output, 'utf8') !== html) throw new Error('Embedded UI is stale; run pnpm build');
} else writeFileSync(output, html);
console.log(`Embedded UI: ${Buffer.byteLength(html)} bytes`);
