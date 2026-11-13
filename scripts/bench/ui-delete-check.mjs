// Check the Clips page Delete workflow in the real REPLAYX window.
//
// Creates a throw-away library with three generated clips (FFmpeg test
// pattern, no screen content) under test-output, launches the given
// replayx.exe on it, selects the middle clip, presses Delete and confirms.
// Then verifies on disk and in the Recycle Bin that exactly that clip moved,
// that the others are untouched and that the list updated. Also checks that a
// clip held open by another program is refused with Windows' reason and kept.
// Nothing is captured. The deleted test clip is left in the Recycle Bin.
// usage: node scripts/bench/ui-delete-check.mjs --exe PATH [--port 9334]
import { spawn, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const arg = (name, fallback) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : fallback; };
const exe = path.resolve(arg('exe'));
const port = Number(arg('port', '9334'));
const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
const out = path.resolve('test-output', `ui-delete-${stamp}`);
const data = path.join(out, 'data');
const clips = path.join(out, 'clips', 'Desktop');
fs.mkdirSync(data, { recursive: true });
fs.mkdirSync(clips, { recursive: true });
const lock = JSON.parse(fs.readFileSync('tools/ffmpeg-lock.json', 'utf8').replace(/^﻿/, ''));
const ffmpeg = path.resolve('tools', lock.runtime_dir, 'ffmpeg.exe');
const names = ['Desktop_2026-11-07_10-00-00.mkv', 'Desktop_2026-11-07_11-00-00.mkv', 'Desktop_2026-11-07_12-00-00.mkv'];
names.forEach((n, i) => {
  execFileSync(ffmpeg, ['-v', 'error', '-f', 'lavfi', '-i', `testsrc2=size=640x360:rate=30:duration=${2 + i}`, '-c:v', 'mpeg4', path.join(clips, n)]);
  const t = new Date(Date.UTC(2026, 10, 7, 10 + i));
  fs.utimesSync(path.join(clips, n), t, t);
});
fs.writeFileSync(path.join(data, 'settings.json'), JSON.stringify({ output_dir: path.join(out, 'clips'), audio: [] }));

const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok, detail }); console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? ` — ${detail}` : ''}`); };
const app = spawn(exe, [], { env: { ...process.env, REPLAYX_DATA_DIR: data, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` }, stdio: 'ignore' });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let wsUrl;
for (let i = 0; i < 200 && !wsUrl; i++) {
  try { wsUrl = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === 'page')?.webSocketDebuggerUrl; } catch { /* starting */ }
  if (!wsUrl) await sleep(100);
}
const ws = new WebSocket(wsUrl);
await new Promise(r => ws.addEventListener('open', r, { once: true }));
let id = 0; const pending = new Map();
ws.addEventListener('message', m => { const msg = JSON.parse(m.data); pending.get(msg.id)?.(msg); pending.delete(msg.id); });
const js = (expression, timeout = 3000) => new Promise(resolve => {
  const n = ++id; const timer = setTimeout(() => { pending.delete(n); resolve('__timeout__'); }, timeout);
  pending.set(n, msg => { clearTimeout(timer); resolve(msg.result?.result?.value); });
  ws.send(JSON.stringify({ id: n, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } }));
});
const until = async (expression, ms = 8000) => { const end = Date.now() + ms; while (Date.now() < end) { if (await js(expression) === true) return true; await sleep(100); } return false; };
const clickText = (selector, text) => js(`(() => { const b = [...document.querySelectorAll(${JSON.stringify(selector)})].find(b => b.textContent.trim().startsWith(${JSON.stringify(text)})); if (!b) return 'missing'; if (b.disabled) return 'disabled'; b.click(); return 'clicked'; })()`);
const cards = `[...document.querySelectorAll('.clip-card')]`;

await until(`!!document.querySelector('.nav-item')`);
await clickText('.nav-item', 'Clips');
check('library lists the three test clips', await until(`${cards}.length === 3`));
// Select the middle clip (11-00-00) by its visible name.
const target = names[1];
await js(`${cards}.find(c => !!c.querySelector('[title*="11-00-00"]'))?.click()`);
check('clicked clip becomes the selected one', await until(`!!${cards}.find(c => c.classList.contains('on'))?.querySelector('[title*="11-00-00"]')`));
check('Delete button responds', (await clickText('.btn', 'Delete')) === 'clicked');
check('confirmation names the selected file', await until(`document.body.textContent.includes('Delete this clip?') && document.querySelector('.modal')?.textContent.includes(${JSON.stringify(target.replace('.mkv', ''))}) === true`));
await clickText('.modal .btn', 'Move to Recycle Bin');
check('clip disappears from the list', await until(`${cards}.length === 2 && !${cards}.some(c => !!c.querySelector('[title*="11-00-00"]'))`));
check('file is gone from the clips folder', !fs.existsSync(path.join(clips, target)));
check('other clips untouched', [names[0], names[2]].every(n => fs.existsSync(path.join(clips, n))));
const bin = execFileSync('powershell', ['-NoProfile', '-Command', `(New-Object -ComObject Shell.Application).NameSpace(10).Items() | Where-Object { $_.Name -like '*11-00-00*' } | ForEach-Object { $_.ExtendedProperty('System.Recycle.DeletedFrom') }`], { encoding: 'utf8' });
check('file is in the Recycle Bin, from the test folder', bin.toLowerCase().includes(path.basename(out).toLowerCase()), bin.trim().split('\n').pop());

// A clip open in another program (no sharing) must be refused and kept.
const locked = path.join(clips, names[2]);
const holder = spawn('powershell', ['-NoProfile', '-Command', `$f=[IO.File]::Open('${locked}','Open','Read','None'); Start-Sleep 20; $f.Close()`]);
await sleep(1500);
await js(`${cards}.find(c => !!c.querySelector('[title*="12-00-00"]'))?.click()`);
await sleep(200);
await clickText('.btn', 'Delete');
await until(`document.body.textContent.includes('Delete this clip?')`);
await clickText('.modal .btn', 'Move to Recycle Bin');
const refused = await until(`!!document.querySelector('.modal') && /open|in use|another program/i.test(document.querySelector('.modal').textContent)`, 6000);
check('a clip in use is refused with a reason, dialog stays open', refused, refused ? await js(`document.querySelector('.modal .banner, .modal .error, .modal [role=alert]')?.textContent ?? ''`) : '');
check('the in-use clip is kept on disk', fs.existsSync(locked));
holder.kill();
await clickText('.modal .btn', 'Cancel');

fs.writeFileSync(path.join(out, 'ui-delete.json'), JSON.stringify(results, null, 2));
ws.close();
try { execFileSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' }); } catch { /* gone */ }
console.log(`${results.filter(r => r.ok).length}/${results.length} passed`);
process.exit(results.every(r => r.ok) ? 0 : 1);
