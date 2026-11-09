// Layout regression check: every page must scroll with the mouse wheel when its
// content is taller than the window, at several window sizes.
// Runs the mocked UI preview in headless Chrome (no visible window, no GPU).
// Needs the preview server: npx vite preview --config dev/vite.preview.ts
// usage: node dev/scroll-check.mjs [--port 9335]
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const port = Number(process.argv[process.argv.indexOf('--port') + 1] || 9335);
// Chrome, not Edge: a new Edge profile signs in with the Windows account and
// syncs the user's extensions and data into it, even with --inprivate and
// --disable-sync (seen 2026-11-08).
const edge = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'rx-scroll-'));
// Flags keep the throwaway profile anonymous; the guard below aborts if any
// page other than the preview appears, and the profile is always deleted.
const browser = spawn(edge, ['--headless=new', '--disable-gpu', '--disable-extensions', '--disable-sync', '--no-first-run',
  '--no-default-browser-check', '--disable-background-networking', '--disable-component-update',
  `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, 'about:blank'], { stdio: ['ignore', 'pipe', 'pipe'] });
browser.stdout.resume(); browser.stderr.resume();
const cleanup = () => {
  try { browser.kill(); } catch { /* gone */ }
  for (let i = 0; i < 20; i++) {
    try { fs.rmSync(profile, { recursive: true, force: true }); return; } catch { Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250); }
  }
};
process.on('exit', cleanup);
browser.on('error', e => { console.log('Browser could not start:', e.message); process.exit(2); });
browser.on('exit', c => { if (!target) console.log('Browser exited early with', c); });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let target;
for (let i = 0; i < 300 && !target; i++) {
  try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === 'page' && t.url === 'about:blank'); } catch { /* starting */ }
  if (!target) await sleep(150);
}
if (!target) { console.log('Browser DevTools endpoint did not appear'); process.exit(2); }
await sleep(1500);
const foreign = (await (await fetch(`http://127.0.0.1:${port}/json`)).json())
  // Allowed: the preview, Chrome's own UI and its two built-in component
  // extensions. Anything else (web pages, user extensions) means the profile
  // is not anonymous.
  .filter(t => !/^(about:blank|http:\/\/127\.0\.0\.1:1430\/|devtools:|chrome:\/\/|chrome-extension:\/\/(nkeimhogjdpnpccoofpliimaahmaaome|fignfifoniblkonapihmkfakmlgkbkcf)\/)/.test(t.url));
if (foreign.length) { console.log(`Aborting: the test browser opened ${foreign.length} unrelated page(s) (account sign-in or extensions): ${foreign.map(t => `${t.type} ${t.url.replace(/^([a-z-]+:\/\/[^/?#]*).*/, '$1')}`).join(', ')}`); process.exit(3); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r => ws.addEventListener('open', r, { once: true }));
let id = 0; const pending = new Map();
ws.addEventListener('message', m => { const msg = JSON.parse(m.data); pending.get(msg.id)?.(msg); pending.delete(msg.id); });
const send = (method, params = {}) => new Promise(resolve => { const n = ++id; pending.set(n, resolve); ws.send(JSON.stringify({ id: n, method, params })); });
const js = async expression => (await send('Runtime.evaluate', { expression, returnByValue: true })).result?.result?.value;

const pages = ['home', 'replay', 'capture', 'audio', 'clips', 'games', 'performance', 'settings'];
const sizes = [[1280, 820], [1000, 640], [820, 560]];
let failures = 0;
for (const [w, h] of sizes) {
  await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: false });
  for (const page of pages) {
    // Full load per page (the preview reads the #page hash at start-up).
    await send('Page.navigate', { url: 'about:blank' });
    await sleep(150);
    await send('Page.navigate', { url: `http://127.0.0.1:1430/dev/preview.html?state=replay&p=${page}#${page}` });
    // Wait until the page has rendered and its height stopped changing.
    let last = -1;
    for (let k = 0; k < 40; k++) {
      await sleep(150);
      const hNow = await js(`document.querySelector('.content')?.scrollHeight ?? -1`);
      if (hNow > 0 && hNow === last) break;
      last = hNow;
    }
    await js(`document.querySelector('.content') && (document.querySelector('.content').scrollTop = 0)`);
    await sleep(100);
    const before = await js(`(() => { const c = document.querySelector('.content'); return c ? { sh: c.scrollHeight, ch: c.clientHeight, top: c.scrollTop, docH: document.documentElement.scrollHeight, winH: innerHeight } : null })()`);
    if (!before) { console.log(`FAIL ${w}x${h} ${page}: no .content`); failures++; continue; }
    await send('Input.dispatchMouseEvent', { type: 'mouseWheel', x: Math.round(w * 0.6), y: Math.round(h * 0.6), deltaX: 0, deltaY: 600 });
    await sleep(400);
    let top = await js(`document.querySelector('.content').scrollTop`);
    if (top === before.top) {
      // Headless Chromium occasionally drops a lone synthetic wheel event.
      await send('Input.dispatchMouseEvent', { type: 'mouseWheel', x: Math.round(w * 0.6), y: Math.round(h * 0.6), deltaX: 0, deltaY: 600 });
      await sleep(400);
      top = await js(`document.querySelector('.content').scrollTop`);
    }
    const needs = before.sh > before.ch + 2;
    // Content taller than the viewport but the container itself not scrollable = the bug.
    const hidden = before.docH > before.winH + 2 && !needs;
    const ok = !hidden && (!needs || top > before.top);
    if (!ok) failures++;
    if (!ok) console.log('   under pointer:', await js(`(() => { let e = document.elementFromPoint(${Math.round(w * 0.6)}, ${Math.round(h * 0.6)}); const path = []; while (e && path.length < 6) { const cs = getComputedStyle(e); path.push(e.tagName.toLowerCase() + (e.className ? '.' + String(e.className).split(' ')[0] : '') + (/(auto|scroll)/.test(cs.overflowY) ? '[scrolls ' + e.scrollHeight + '/' + e.clientHeight + ']' : '')); e = e.parentElement; } return path.join(' < '); })()`));
    console.log(`${ok ? 'PASS' : 'FAIL'} ${w}x${h} ${page.padEnd(12)} content ${before.sh}/${before.ch}px, wheel moved ${top - before.top}px${hidden ? ', content clipped below the window' : ''}`);
  }
}
ws.close();
console.log(failures ? `${failures} failure(s)` : 'all pages scroll');
process.exit(failures ? 1 : 0);
