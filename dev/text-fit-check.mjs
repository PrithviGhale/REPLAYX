// Layout regression check: no text may be cut off by its box (clipped by an
// `overflow: hidden` ancestor or running past a fixed-width element) on any
// page at common window sizes, including a restored (non-maximised) window.
// Text that ends in an intentional ellipsis is listed separately.
// Runs the mocked UI preview in headless Chrome (anonymous throwaway profile).
// Needs the preview server: npx vite preview --config dev/vite.preview.ts --port 1430
// usage: node dev/text-fit-check.mjs [--port 9337]
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const port = Number(process.argv[process.argv.indexOf('--port') + 1] || 9337);
// Chrome, not Edge: a new Edge profile signs in with the Windows account.
const chrome = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'rx-fit-'));
const browser = spawn(chrome, ['--headless=new', '--disable-gpu', '--disable-extensions', '--disable-sync', '--no-first-run',
  '--no-default-browser-check', '--disable-background-networking', '--disable-component-update',
  `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, 'about:blank'], { stdio: ['ignore', 'pipe', 'pipe'] });
browser.stdout.resume(); browser.stderr.resume();
process.on('exit', () => {
  try { browser.kill(); } catch { /* gone */ }
  for (let i = 0; i < 20; i++) {
    try { fs.rmSync(profile, { recursive: true, force: true }); return; } catch { Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250); }
  }
});
const sleep = ms => new Promise(r => setTimeout(r, ms));
let target;
for (let i = 0; i < 300 && !target; i++) {
  try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === 'page' && t.url === 'about:blank'); } catch { /* starting */ }
  if (!target) await sleep(150);
}
if (!target) { console.log('Browser DevTools endpoint did not appear'); process.exit(2); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r => ws.addEventListener('open', r, { once: true }));
let id = 0; const pending = new Map();
ws.addEventListener('message', m => { const msg = JSON.parse(m.data); pending.get(msg.id)?.(msg); pending.delete(msg.id); });
const send = (method, params = {}) => new Promise(resolve => { const n = ++id; pending.set(n, resolve); ws.send(JSON.stringify({ id: n, method, params })); });
const js = async expression => (await send('Runtime.evaluate', { expression, returnByValue: true })).result?.result?.value;

// In-page probe: every element that directly holds text.
const probe = `(() => {
  const cut = [], ellipsis = [];
  const label = e => (e.tagName.toLowerCase() + (e.className && typeof e.className === 'string' ? '.' + e.className.trim().split(/\\s+/)[0] : ''));
  const text = e => [...e.childNodes].filter(n => n.nodeType === 3).map(n => n.textContent).join('').trim();
  for (const e of document.querySelectorAll('body *')) {
    const t = text(e);
    if (!t || e.closest('[aria-hidden="true"], option, script, style, .sr-only')) continue;
    const cs = getComputedStyle(e);
    if (cs.display === 'none' || cs.visibility === 'hidden') continue;
    const r = e.getBoundingClientRect();
    if (!r.width || !r.height) continue;
    // Text wider than its own clipping box.
    if (e.scrollWidth > e.clientWidth + 1 && cs.overflowX !== 'visible') {
      (cs.textOverflow === 'ellipsis' ? ellipsis : cut).push(label(e) + ' "' + t.slice(0, 50) + '"');
      continue;
    }
    // Text spilling out of the card, button or badge around it (painted
    // under a neighbour or past a border), even when nothing clips it.
    const boxed = a => { const st = getComputedStyle(a); return parseFloat(st.borderRightWidth) > 0 || !(st.backgroundColor === 'transparent' || st.backgroundColor.replace(/ /g, '') === 'rgba(0,0,0,0)'); };
    let spilled = false;
    for (let a = e.parentElement; a && a !== document.body; a = a.parentElement) {
      if (/(auto|scroll)/.test(getComputedStyle(a).overflowX)) break;
      if (!boxed(a)) continue;
      const ar = a.getBoundingClientRect();
      const range = document.createRange(); range.selectNodeContents(e);
      const tr = range.getBoundingClientRect();
      if (tr.right > ar.right + 1 || tr.left < ar.left - 1) { cut.push(label(e) + ' "' + t.slice(0, 50) + '" spills out of ' + label(a)); spilled = true; }
      break;
    }
    if (spilled) continue;
    // A box running past an ancestor that clips (and doesn't scroll).
    for (let a = e.parentElement; a && a !== document.body; a = a.parentElement) {
      const as = getComputedStyle(a);
      if (as.overflowX === 'visible' && as.overflowY === 'visible') continue;
      if (/(auto|scroll)/.test(as.overflowX + as.overflowY)) break;
      const ar = a.getBoundingClientRect();
      if (r.right > ar.right + 1 || r.left < ar.left - 1 || r.bottom > ar.bottom + 1) {
        cut.push(label(e) + ' "' + t.slice(0, 50) + '" clipped by ' + label(a));
      }
      break;
    }
  }
  return { cut: [...new Set(cut)], ellipsis: [...new Set(ellipsis)] };
})()`;

const pages = ['home', 'replay', 'capture', 'audio', 'clips', 'games', 'performance', 'settings'];
const tabs = { settings: ['general', 'hotkeys', 'updates', 'about'] };
const sizes = [[1280, 820], [1180, 760], [1100, 720], [1000, 680], [900, 640], [820, 560]];
// Windows "Text size" (Accessibility) and display scaling make text larger
// relative to the layout; the 130% pass enlarges every font without changing
// box sizes, the harsher case.
const textScales = [1, 1.3];
const enlarge = f => `(() => { const all = [...document.querySelectorAll('body *:not([data-big])')]; const sizes = all.map(e => parseFloat(getComputedStyle(e).fontSize)); all.forEach((e, i) => { e.style.fontSize = (sizes[i] * ${f}) + 'px'; e.dataset.big = ''; }); })()`;
let failures = 0;
const truncated = new Set();
for (const scale of textScales) for (const [w, h] of sizes) {
  await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: false });
  for (const page of pages) {
    await send('Page.navigate', { url: 'about:blank' });
    await sleep(120);
    await send('Page.navigate', { url: `http://127.0.0.1:1430/dev/preview.html?state=replay&p=${page}#${page}` });
    let last = -1;
    for (let k = 0; k < 40; k++) {
      await sleep(150);
      const hNow = await js(`document.querySelector('.content')?.scrollHeight ?? -1`);
      if (hNow > 0 && hNow === last) break;
      last = hNow;
    }
    for (const tab of tabs[page] ?? [null]) {
      if (tab) {
        await js(`[...document.querySelectorAll('[role=tab]')].find(b => b.textContent.trim().toLowerCase() === '${tab}')?.click()`);
        await sleep(400);
      }
      if (scale !== 1) { await js(enlarge(scale)); await sleep(150); }
      const r = await js(probe);
      const where = `${w}x${h}${scale !== 1 ? ` text ${Math.round(scale * 100)}%` : ''} ${page}${tab ? '/' + tab : ''}`;
      r.ellipsis.forEach(x => truncated.add(`${page}: ${x}`));
      if (r.cut.length) { failures += r.cut.length; console.log(`FAIL ${where}:`); r.cut.forEach(x => console.log('   ' + x)); }
      else console.log(`PASS ${where}`);
    }
  }
}
if (truncated.size) { console.log('\nShortened with an ellipsis (by design; full text on hover):'); [...truncated].forEach(x => console.log('   ' + x)); }
ws.close();
console.log(failures ? `\n${failures} clipped text element(s)` : '\nno clipped text');
process.exit(failures ? 1 : 0);
