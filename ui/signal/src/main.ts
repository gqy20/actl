import './styles.css';
import { parseSnapshot, summaryLines, type Snapshot } from './model.ts';
import { disclosureMotion } from './disclosure.ts';

interface NativeBridge {
  postMessage(message: { version: 1; kind: 'ready' | 'height'; height?: number }): void;
  addEventListener(type: 'message', listener: (event: { data: unknown }) => void): void;
}
declare global { interface Window { chrome?: { webview?: NativeBridge } } }
const bridge = window.chrome?.webview;
const element = <T extends HTMLElement>(id: string): T => {
  const node = document.getElementById(id);
  if (!node) throw new Error(`Missing element: ${id}`);
  return node as T;
};
const content = element('content');
let lastHeight = 0;
function emitHeight(height: number): void {
  if (height !== lastHeight) {
    lastHeight = height;
    bridge?.postMessage({ version: 1, kind: 'height', height });
  }
}
const motion = disclosureMotion(content, emitHeight);
let heightFrame = 0;
function reportHeight(): void {
  if (!bridge || heightFrame || motion.isAnimating()) return;
  heightFrame = requestAnimationFrame(() => {
    heightFrame = 0;
    if (!motion.isAnimating()) emitHeight(Math.ceil(content.getBoundingClientRect().height));
  });
}
function setText(id: string, text: string): void {
  const node = element(id);
  if (node.textContent !== text) node.textContent = text;
}
function render(snapshot: Snapshot): void {
  setText('status', snapshot.status);
  element('status').dataset.tone = snapshot.tone;
  const summary = element('summary');
  if (summary.dataset.body !== snapshot.body) {
    const lines = summaryLines(snapshot.body);
    const hasProgress = /\d+\s*\/\s*\d+/.test(lines[0] ?? '');
    summary.replaceChildren(...lines.map((line, index) => {
      const node = document.createElement('p');
      node.className = hasProgress && index === 0 ? 'metrics' : hasProgress && index === 1 ? 'current-step' : 'reason';
      node.textContent = line;
      return node;
    }));
    summary.dataset.body = snapshot.body;
  }
  setText('records', snapshot.records);
  setText('diagnostics', snapshot.diagnostics);
  setText('feedback', snapshot.feedback);
  element('feedback').hidden = snapshot.feedback.length === 0;
  setText('preview-title', snapshot.title);
  setText('preview-primary', snapshot.primary);
  const primary = element<HTMLButtonElement>('preview-primary');
  primary.hidden = snapshot.primary.length === 0;
  primary.disabled = !snapshot.primaryEnabled;
  setText('preview-stop', snapshot.stop);
  setText('preview-shortcut', snapshot.shortcut);
  reportHeight();
}
new ResizeObserver(reportHeight).observe(content);
if (bridge) {
  bridge.addEventListener('message', event => {
    const snapshot = parseSnapshot(event.data);
    if (snapshot) render(snapshot);
  });
  bridge.postMessage({ version: 1, kind: 'ready' });
} else if (import.meta.env.DEV) {
  document.body.classList.add('preview');
  for (const id of ['preview-tools', 'preview-header', 'preview-footer']) element(id).hidden = false;
  import('./mocks.ts').then(({ mocks }) => {
    const tools = element('preview-tools');
    for (const [name, snapshot] of Object.entries(mocks)) {
      const button = document.createElement('button');
      button.textContent = { paused: '暂停', running: '运行', completed: '完成', review: '待核对', unknown: '未知', stopped: '急停', long: '长内容' }[name] ?? name;
      button.addEventListener('click', () => {
        tools.querySelectorAll('button').forEach(node => node.setAttribute('aria-pressed', String(node === button)));
        render(snapshot);
        history.replaceState(null, '', `?state=${name}`);
      });
      tools.append(button);
    }
    const initial = new URLSearchParams(location.search).get('state') ?? 'paused';
    render(mocks[initial] ?? mocks.paused);
  });
}
