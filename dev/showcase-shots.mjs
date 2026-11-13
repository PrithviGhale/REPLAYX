// README showcase images: app screenshots (from dev/readme-shots.mjs) tilted
// in 3D with a glowing edge and a reflection, rendered by dev/showcase.html in
// an anonymous headless Chrome. No server needed.
// usage: node dev/showcase-shots.mjs [outDir]   (run dev/readme-shots.mjs first)
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.resolve(process.argv[2] || path.join(root, 'docs/images'));
const port = 9362;
const chrome = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'rx-showcase-'));
const browser = spawn(chrome, ['--headless=new', '--hide-scrollbars', '--disable-extensions', '--disable-sync', '--no-first-run',
  '--no-default-browser-check', '--disable-background-networking', '--disable-component-update', '--allow-file-access-from-files',
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

await send('Emulation.setDeviceMetricsOverride', { width: 1600, height: 900, deviceScaleFactor: 1.5, mobile: false });
const stage = pathToFileURL(path.join(root, 'dev/showcase.html')).href;
const img = name => `../docs/images/${name}`;
const shots = [
  { file: 'showcase-home.jpg', front: img('home.png'), back: img('clips.png'), tilt: 'right' },
  { file: 'showcase-replay.jpg', front: img('replay.png'), tilt: 'left' },
  { file: 'showcase-library.jpg', front: img('clips.png'), back: img('games.png'), tilt: 'right' },
];
fs.mkdirSync(out, { recursive: true });
for (const s of shots) {
  const q = new URLSearchParams({ front: s.front, tilt: s.tilt, ...(s.back ? { back: s.back } : {}) });
  await send('Page.navigate', { url: `${stage}?${q}` });
  for (let i = 0; i < 50 && !(await js('document.body.dataset.ready')); i++) await sleep(100);
  await sleep(400);
  const shot = await send('Page.captureScreenshot', { format: 'jpeg', quality: 90 });
  fs.writeFileSync(path.join(out, s.file), Buffer.from(shot.result.data, 'base64'));
  console.log('saved', s.file);
}
ws.close();
process.exit(0);
