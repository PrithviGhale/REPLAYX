// Startup timing of a built replayx.exe: launch (tray only, --background),
// read timings from inside the WebView through its DevTools port, close.
// usage: node dev/startup-bench.mjs <path-to-replayx.exe> [runs=4] [port=9410]
// Measures, from process launch: page start (WebView ready to load), app
// mounted (.shell in the DOM), and ready (the app's own ready mark, when the
// build has one). Close REPLAYX first: a running instance would just be focused.
import { spawn, execFileSync } from 'node:child_process';

const exe = process.argv[2];
const runs = Number(process.argv[3] || 4);
const port = Number(process.argv[4] || 9410);
if (!exe) { console.log('usage: node dev/startup-bench.mjs <replayx.exe> [runs] [port]'); process.exit(2); }
const sleep = ms => new Promise(r => setTimeout(r, ms));

async function once(run) {
  const t0 = Date.now();
  const child = spawn(exe, ['--background'], {
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
    stdio: 'ignore', detached: false,
  });
  let target;
  for (let i = 0; i < 400 && !target; i++) {
    try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === 'page'); } catch { /* starting */ }
    if (!target) await sleep(25);
  }
  if (!target) { child.kill(); throw new Error('WebView DevTools did not appear'); }
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise(r => ws.addEventListener('open', r, { once: true }));
  let id = 0; const pending = new Map();
  ws.addEventListener('message', m => { const msg = JSON.parse(m.data); pending.get(msg.id)?.(msg); pending.delete(msg.id); });
  const js = expression => new Promise(resolve => { const n = ++id; pending.set(n, msg => resolve(msg.result?.result?.value)); ws.send(JSON.stringify({ id: n, method: 'Runtime.evaluate', params: { expression, returnByValue: true } })); });
  let r = null;
  for (let i = 0; i < 600; i++) {
    r = await js(`(() => { const shell = !!document.querySelector('.shell'); window.__mounted ??= shell ? Math.round(performance.now()) : undefined;
      return { origin: performance.timeOrigin, mounted: window.__mounted ?? null, ready: window.__replayxReady ?? null, splash: !!document.getElementById('splash') }; })()`);
    if (r && r.mounted !== null && (r.ready !== null || i > 40 && !r.splash)) break;
    await sleep(20);
  }
  ws.close();
  try { execFileSync('taskkill', ['/F', '/T', '/PID', String(child.pid)], { stdio: 'ignore' }); } catch { /* gone */ }
  await sleep(1500);
  const start = Math.round(r.origin - t0);
  const line = { run, page_start_ms: start, mounted_ms: r.mounted === null ? null : start + r.mounted, ready_ms: r.ready === null ? null : start + r.ready };
  console.log(JSON.stringify(line));
  return line;
}

const all = [];
for (let i = 1; i <= runs; i++) all.push(await once(i));
const median = k => { const v = all.map(x => x[k]).filter(x => x !== null).sort((a, b) => a - b); return v.length ? v[Math.floor(v.length / 2)] : null; };
console.log(JSON.stringify({ exe, runs, median_page_start_ms: median('page_start_ms'), median_mounted_ms: median('mounted_ms'), median_ready_ms: median('ready_ms') }));
