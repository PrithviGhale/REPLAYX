// README screenshots of the mocked UI preview (sample data, no real clips or
// accounts), taken in an anonymous headless Chrome at 1.5x for sharp text.
// Needs the preview server: npx vite preview --config dev/vite.preview.ts --port 1430
// usage: node dev/readme-shots.mjs [outDir]
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const out = path.resolve(process.argv[2] || 'docs/images');
const port = 9361;
const chrome = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'rx-shots-'));
const browser = spawn(chrome, ['--headless=new', '--disable-gpu', '--hide-scrollbars', '--disable-extensions', '--disable-sync', '--no-first-run',
  '--no-default-browser-check', '--disable-background-networking', '--disable-component-update', '--force-prefers-reduced-motion',
  `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, 'about:blank'], { stdio: 'ignore' });
process.on('exit', () => { try { browser.kill(); } catch { /* gone */ } try { fs.rmSync(profile, { recursive: true, force: true }); } catch { /* busy */ } });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let target;
for (let i = 0; i < 300 && !target; i++) {
  try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === 'page'); } catch { /* starting */ }
  if (!target) await sleep(150);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r => ws.addEventListener('open', r, { once: true }));
let id = 0; const pending = new Map();
ws.addEventListener('message', m => { const msg = JSON.parse(m.data); pending.get(msg.id)?.(msg); pending.delete(msg.id); });
const send = (method, params = {}) => new Promise(resolve => { const n = ++id; pending.set(n, resolve); ws.send(JSON.stringify({ id: n, method, params })); });
const js = async expression => (await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;

const W = 1280, H = 800;
await send('Emulation.setDeviceMetricsOverride', { width: W, height: H, deviceScaleFactor: 1.5, mobile: false });
const clickText = (sel, text) => js(`[...document.querySelectorAll('${sel}')].find(b => b.textContent.trim().startsWith(${JSON.stringify(text)}))?.click()`);
const scrollTo = text => js(`(() => { const h = [...document.querySelectorAll('.card h2, .card h3, .card-title, h2, h3')].find(x => x.textContent.trim().startsWith(${JSON.stringify(text)})); const c = h?.closest('section, .card') ?? h; c?.scrollIntoView({ block: 'start' }); document.querySelector('.content').scrollTop -= 16; })()`);

const shots = [
  { file: 'home.png', url: 'state=replay#home' },
  { file: 'replay.png', url: 'state=replay#replay' },
  { file: 'video.png', url: 'state=off#capture' },
  { file: 'audio.png', url: 'state=off#audio' },
  { file: 'clips.png', url: 'state=off#clips' },
  { file: 'clips-by-game.png', url: 'state=off#clips', then: () => clickText('button', 'By game') },
  { file: 'games.png', url: 'state=off#games' },
  { file: 'performance.png', url: 'state=replay#performance' },
  { file: 'settings-overlay.png', url: 'state=off#settings', then: () => scrollTo('Status overlay') },
  { file: 'hotkeys.png', url: 'state=off#settings', then: () => clickText('[role=tab]', 'Hotkeys') },
  { file: 'tour.png', url: 'state=off&tour#home', wait: 2500 },
];
fs.mkdirSync(out, { recursive: true });
for (const s of shots) {
  await send('Page.navigate', { url: 'about:blank' });
  await sleep(150);
  await send('Page.navigate', { url: `http://127.0.0.1:1430/dev/preview.html?${s.url}` });
  await sleep(s.wait ?? 1800);
  if (s.then) { await s.then(); await sleep(700); }
  const shot = await send('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(path.join(out, s.file), Buffer.from(shot.result.data, 'base64'));
  console.log('saved', s.file);
}
ws.close();
process.exit(0);
