// Drive the real REPLAYX window like a user and measure whether it stays responsive.
//
// Launches the given replayx.exe against a throw-away data folder (your real
// settings, clips and sessions are never touched), enables WebView2 remote
// debugging on 127.0.0.1 to click buttons inside the page, and runs
// scripts/window-check.py next to it to measure how fast the window answers
// Windows messages. Captures the selected display silently (no audio sources).
//
// usage: node scripts/ui-record-check.mjs --exe PATH [--label NAME]
//        [--steps "record,wait:8,stop,wait:4"] [--port 9333]
// Steps: record | stop | cancel | replay | save | replay-off | wait:SECONDS
import { spawn, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : fallback;
};
const exe = path.resolve(arg('exe'));
const label = arg('label', 'run');
const steps = arg('steps', 'record,wait:8,stop,wait:4').split(',');
const port = Number(arg('port', '9333'));
const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
const out = path.resolve('test-output', `ui-check-${stamp}-${label}`);
const data = path.join(out, 'data');
fs.mkdirSync(data, { recursive: true });
fs.writeFileSync(path.join(data, 'settings.json'), JSON.stringify({
  output_dir: path.join(out, 'clips'), height: 1080, canvas_mode: 'native', fps: 60, audio: [],
}, null, 2));

const log = [];
const t0 = performance.now();
const note = (event, extra = {}) => {
  const e = { t: Math.round(performance.now() - t0), event, ...extra };
  log.push(e);
  console.log(JSON.stringify(e));
};

const app = spawn(exe, [], {
  env: { ...process.env, REPLAYX_DATA_DIR: data, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
  stdio: 'ignore',
});
note('launched', { pid: app.pid, exe });

const sleep = ms => new Promise(r => setTimeout(r, ms));
async function target() {
  for (let i = 0; i < 200; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = list.find(t => t.type === 'page');
      if (page) return page.webSocketDebuggerUrl;
    } catch { /* not up yet */ }
    await sleep(100);
  }
  throw new Error('WebView2 debugging endpoint did not appear');
}
const ws = new WebSocket(await target());
await new Promise(r => ws.addEventListener('open', r, { once: true }));
let id = 0;
const pending = new Map();
ws.addEventListener('message', m => {
  const msg = JSON.parse(m.data);
  if (pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
});
// The page stops answering while the host window is hung: never wait forever.
const evaluate = (expression, timeout = 2000) => new Promise(resolve => {
  const n = ++id;
  const timer = setTimeout(() => { pending.delete(n); resolve('__timeout__'); }, timeout);
  pending.set(n, msg => { clearTimeout(timer); resolve(msg.result?.result?.value); });
  ws.send(JSON.stringify({ id: n, method: 'Runtime.evaluate', params: { expression, returnByValue: true } }));
});
// Hung = the window has not answered WM_NULL for more than 5 s (Windows shows "Not responding").
const hungFor = () => {
  let since = null;
  for (const s of samples) {
    if (s.answered === false) since ??= s.at; else if (s.answered) since = null;
  }
  return since === null ? 0 : performance.now() - t0 - since;
};
// Wait for the UI to load.
for (let i = 0; i < 100 && !(await evaluate(`!!document.querySelector('.dock button')`)); i++) await sleep(100);
note('ui-ready');

const monitor = spawn('python', [path.resolve('scripts/window-check.py'), '--pid', String(app.pid), '--seconds', '600', '--foreground', '--keep-foreground']);
const samples = [];
monitor.stdout.on('data', chunk => {
  for (const line of chunk.toString().split('\n').filter(Boolean)) {
    const s = JSON.parse(line);
    s.at = Math.round(performance.now() - t0);
    samples.push(s);
  }
});
await sleep(1500);

const pill = () => evaluate(`document.querySelector('.state-pill')?.textContent ?? ''`);
const click = text => evaluate(`(() => { const b = [...document.querySelectorAll('.dock button')].find(b => b.textContent.trim().startsWith(${JSON.stringify(text)})); if (!b) return 'missing'; if (b.disabled) return 'disabled'; b.click(); return 'clicked'; })()`);
let aborted = false;
async function watchPill(ms) {
  // Record every state change the user would see, every 50 ms.
  const end = performance.now() + ms;
  let last = null;
  while (performance.now() < end) {
    if (hungFor() > 5000) { note('hang', { unresponsive_ms: Math.round(hungFor()) }); aborted = true; return; }
    const p = await pill();
    if (p !== last) { note('pill', { text: p }); last = p; }
    await sleep(50);
  }
}
const buttonFor = { record: 'Record', stop: 'Stop', cancel: 'Cancel', replay: 'Enable replay', 'replay-off': 'Replay on', save: 'Save replay' };
for (const step of steps) {
  if (aborted) break;
  if (step.startsWith('wait:')) { await watchPill(Number(step.slice(5)) * 1000); continue; }
  note('click', { step, result: await click(buttonFor[step]) });
  await watchPill(300);
}
const toasts = aborted ? [] : await evaluate(`[...document.querySelectorAll('.toast')].map(t => t.textContent)`);
note('toasts', { toasts });

monitor.kill();
const worst = samples.reduce((m, s) => Math.max(m, s.latency_ms ?? 0), 0);
const unanswered = samples.filter(s => s.answered === false).length;
const hung = samples.filter(s => s.hung).length;
const foreground = samples.filter(s => s.foreground).length;
const summary = {
  label, exe, steps, samples: samples.length, worst_message_latency_ms: worst,
  unanswered_1s: unanswered, hung_samples: hung, foreground_samples: foreground,
  longest_unresponsive_ms: Math.round(hungFor()),
  verdict: aborted || unanswered > 0 || hung > 0 ? 'NOT RESPONDING' : 'responsive',
};
note('summary', summary);
fs.writeFileSync(path.join(out, 'ui-check.json'), JSON.stringify({ summary, log, samples }, null, 2));
ws.close();
// Close: a hung window cannot be asked, so end the process tree (the test data folder is throw-away).
try { execFileSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' }); } catch { /* already gone */ }
process.exit(0);
