// End-to-end test of the updater and the microphone switch in the real
// development app (`tauri dev`), without GitHub and without installing anything.
//
// - A local server (127.0.0.1) plays GitHub: it serves latest.json and a dummy
//   "installer" in many variants (valid, tampered, wrong key, older version,
//   no Windows build, server errors, a hanging server, offline).
// - The app runs the real update code (Tauri updater: check, download, verify
//   the signature with the built-in public key). Development builds only use
//   the test server when REPLAYX_UPDATE_TEST_ENDPOINT is set; release builds
//   ignore it. Installing is refused in development builds, so the installed
//   REPLAYX is never touched.
// - The app uses a temporary data folder (REPLAYX_DATA_DIR): your settings,
//   clips and playtime are not read or changed. Capture is never started, so
//   the microphone is never opened.
//
// Needs the update signing key (to sign the dummy payload):
//   %USERPROFILE%\.replayx-signing\replayx-updater.key and password.txt
// usage: node dev/update-e2e.mjs        (close REPLAYX first: one copy at a time)
import { spawn, spawnSync, execSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sleep = ms => new Promise(r => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok }); console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  (${detail})` : ''}`); };

if (execSync('tasklist /FI "IMAGENAME eq replayx.exe" /NH', { encoding: 'utf8' }).includes('replayx.exe')) {
  console.log('REPLAYX is running. Quit it first (tray icon, Quit): only one copy can run.'); process.exit(2);
}

// ---------------------------------------------------------------- fixtures
fs.mkdirSync(path.join(root, 'test-output'), { recursive: true });
const work = fs.mkdtempSync(path.join(root, 'test-output', 'update-e2e-'));
const dataDir = path.join(work, 'data');
fs.mkdirSync(dataDir, { recursive: true });
fs.writeFileSync(path.join(dataDir, 'settings.json'), JSON.stringify({ tour_done: true, tray_hint_shown: true, output_dir: path.join(work, 'clips') }));
const current = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).version;
const bump = v => v.replace(/(\d+)$/, n => String(Number(n) + 1));
const lower = v => v.replace(/(\d+)$/, n => String(Math.max(0, Number(n) - 1)));
const [newer, older] = [bump(current), lower(current)];
const signDir = path.join(os.homedir(), '.replayx-signing');
const signEnv = { ...process.env, TAURI_SIGNING_PRIVATE_KEY_PATH: path.join(signDir, 'replayx-updater.key'), TAURI_SIGNING_PRIVATE_KEY_PASSWORD: fs.readFileSync(path.join(signDir, 'password.txt'), 'utf8') };
function signed(name, bytes, version) {
  const file = path.join(work, name);
  fs.writeFileSync(file, bytes);
  const r = spawnSync(process.execPath, [path.join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'signer', 'sign', '--app-version', version, file], { cwd: root, env: signEnv, encoding: 'utf8' });
  if (r.status !== 0 || !fs.existsSync(`${file}.sig`)) { console.log('Could not sign the test payload.'); process.exit(2); }
  return fs.readFileSync(`${file}.sig`, 'utf8').trim();
}
// A dummy payload (random bytes, never executed): 3 MB so progress is visible.
const payload = crypto.randomBytes(3 * 1024 * 1024);
const tampered = Buffer.from(payload); tampered[12345] ^= 1;
const sigNewer = signed('payload-newer.bin', payload, newer);
const sigOld = signed('payload-old.bin', payload, older);
// A signature in the same format from a key that is not REPLAYX's.
const strangerSig = (() => {
  const { privateKey } = crypto.generateKeyPairSync('ed25519');
  const id = crypto.randomBytes(8), trusted = `timestamp:1\tfile:x.exe\tversion:${newer}`;
  const sig = crypto.sign(null, crypto.createHash('blake2b512').update(payload).digest(), privateKey);
  const global = crypto.sign(null, Buffer.concat([sig, Buffer.from(trusted)]), privateKey);
  return Buffer.from(`untrusted comment: signature from tauri secret key\n${Buffer.concat([Buffer.from('ED'), id, sig]).toString('base64')}\ntrusted comment: ${trusted}\n${global.toString('base64')}\n`).toString('base64');
})();

// ---------------------------------------------------------------- test server
let scenario = 'newer';
const hits = { manifest: 0, payload: 0 };
const server = http.createServer(async (req, res) => {
  const base = `http://127.0.0.1:${server.address().port}`;
  if (req.url.startsWith('/latest.json')) {
    hits.manifest++;
    const manifest = (version, signature, file = 'payload.bin', platform = 'windows-x86_64') => JSON.stringify({
      version, notes: "## What's New\n\n- TEST DATA from the local test server\n- Second line\n- Third line", pub_date: '2026-11-11T12:00:00Z',
      platforms: { [platform]: { signature, url: `${base}/files/${file}` } },
    });
    const send = (code, body, type = 'application/json') => { res.writeHead(code, { 'content-type': type }); res.end(body); };
    switch (scenario) {
      case 'newer': return send(200, manifest(newer, sigNewer));
      case 'slowmanifest': await sleep(1500); return send(200, manifest(newer, sigNewer));
      case 'slow': return send(200, manifest(newer, sigNewer, 'slow.bin'));
      case 'tampered': return send(200, manifest(newer, sigNewer, 'tampered.bin'));
      case 'interrupted': return send(200, manifest(newer, sigNewer, 'interrupted.bin'));
      case 'wrongkey': return send(200, manifest(newer, strangerSig));
      case 'oldsig': return send(200, manifest(newer, sigOld));
      case 'older': return send(200, manifest(older, sigOld));
      case 'same': return send(200, manifest(current, sigNewer));
      case 'noarch': return send(200, manifest(newer, sigNewer, 'payload.bin', 'darwin-aarch64'));
      case 'badjson': return send(200, '{ this is not json');
      case 'e500': return send(500, 'server error', 'text/plain');
      case 'e404': return send(404, 'not found', 'text/plain');
      case 'hang': return; // never answers: the app's 20 s timeout must end the check
      default: return send(404, 'unknown scenario', 'text/plain');
    }
  }
  if (req.url.startsWith('/files/')) {
    hits.payload++;
    const bytes = req.url.includes('tampered') ? tampered : payload;
    res.writeHead(200, { 'content-type': 'application/octet-stream', 'content-length': bytes.length });
    if (req.url.includes('interrupted')) { res.write(bytes.subarray(0, 1024)); await sleep(100); return res.destroy(); }
    if (!req.url.includes('slow')) return res.end(bytes);
    for (let i = 0; i < bytes.length; i += 256 * 1024) { res.write(bytes.subarray(i, i + 256 * 1024)); await sleep(120); }
    return res.end();
  }
  res.writeHead(404); res.end();
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const port = server.address().port;

// ---------------------------------------------------------------- the app
const cdp = 9560 + Math.floor(Math.random() * 30);
const log = fs.createWriteStream(path.join(work, 'tauri-dev.log'));
const app = spawn(process.execPath, [path.join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'dev'], {
  cwd: root, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, REPLAYX_DATA_DIR: dataDir, REPLAYX_UPDATE_TEST_ENDPOINT: `http://127.0.0.1:${port}/latest.json`, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${cdp} --remote-debugging-address=127.0.0.1` },
});
app.stdout.pipe(log); app.stderr.pipe(log);
const stopApp = () => { try { execSync(`taskkill /PID ${app.pid} /T /F`, { stdio: 'ignore' }); } catch { /* gone */ } };
process.on('exit', stopApp);
const startedAt = Date.now();
let target;
for (let i = 0; i < 2400 && !target; i++) { // the first debug build can take many minutes
  try { target = (await (await fetch(`http://127.0.0.1:${cdp}/json`)).json()).find(t => t.type === 'page' && t.url.startsWith('http://localhost:1420')); } catch { /* starting */ }
  if (!target) await sleep(500);
}
if (!target) { console.log('The development app did not start (see', path.join(work, 'tauri-dev.log') + ').'); process.exit(2); }
console.log(`app ready after ${Math.round((Date.now() - startedAt) / 1000)} s; test server on port ${port}; data folder ${dataDir}`);
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r => ws.addEventListener('open', r, { once: true }));
let id = 0; const pending = new Map();
ws.addEventListener('message', m => { const x = JSON.parse(m.data); pending.get(x.id)?.(x); pending.delete(x.id); });
const js = expression => new Promise(resolve => { const n = ++id; pending.set(n, x => resolve(x.result?.result?.value ?? { __error: x.result?.exceptionDetails?.exception?.description ?? 'no value' })); ws.send(JSON.stringify({ id: n, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } })); });
const invoke = (cmd, args = {}) => js(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}).then(v => v === undefined || v === null ? { __ok: true } : v, e => ({ __rejected: String(e) }))`);
const status = () => invoke('update_status');
const until = async (what, test, timeout = 30000) => { const end = Date.now() + timeout; let s; do { s = await status(); if (test(s)) return s; await sleep(100); } while (Date.now() < end); console.log(`   timed out waiting for ${what}; last status:`, JSON.stringify(s)); return s; };
const use = s => { scenario = s; };
for (let i = 0; i < 100; i++) { if (await js(`!!window.__TAURI_INTERNALS__ && !!document.querySelector('nav')`) === true) break; await sleep(200); }

// ---------------------------------------------------------------- tests
let s = await status();
check('status at start: update channel configured, development build cannot install', s.configured === true && s.installable === false, `current ${s.current}`);

// 1. Startup check happens by itself, about 20 s after start (never during start-up).
s = await until('the automatic startup check', x => x.stage === 'available', 45000);
check('automatic check after start-up finds the newer release', s.stage === 'available' && s.latest === newer && hits.manifest === 1, `latest ${s.latest}, ${hits.manifest} request`);
check('the result is labelled as test data and offers the pop-up', s.simulated === true && s.prompt === true && typeof s.last_checked === 'number');
check('release notes arrive with the update', (s.notes ?? '').includes('TEST DATA from the local test server'));

// 2. Interface: Settings dot and pop-up follow the engine state.
await sleep(600);
check('Settings shows the update dot', await js(`!!document.querySelector('.nav-dot')`) === true);
check('the update pop-up is shown with the version', await js(`(document.querySelector('.update-pop')?.textContent ?? '').includes(${JSON.stringify(newer)})`) === true);
await js(`[...document.querySelectorAll('.update-pop button')].find(b => b.textContent.trim() === 'Not Now')?.click()`);
await sleep(500);
s = await status();
check('"Not Now" hides the pop-up; the dot stays', s.prompt === false && await js(`!document.querySelector('.update-pop') && !!document.querySelector('.nav-dot')`) === true);

// 3. Manual check; dismissal is remembered for the same version.
s = await invoke('check_update');
check('manual check: same version stays dismissed', s.stage === 'available' && s.prompt === false);
const memory = JSON.parse(fs.readFileSync(path.join(dataDir, 'updates.json'), 'utf8'));
check('dismissal and last check are saved to disk', memory.dismissed_version === newer && typeof memory.last_checked === 'number');

// 4. No duplicate concurrent checks.
use('slowmanifest'); const before = hits.manifest;
await Promise.all([invoke('check_update'), invoke('check_update'), invoke('check_update')]);
check('three simultaneous checks make one request', hits.manifest - before === 1, `${hits.manifest - before} request`);

// 5. Download with real progress, then signature verification.
use('slow'); await invoke('check_update');
const seen = new Set(); const stages = new Set(); const payloadBefore = hits.payload;
await invoke('update_download', { install: false });
await invoke('update_download', { install: false }); // a second click must not start a second download
for (let i = 0; i < 400; i++) { s = await status(); stages.add(s.stage); if (s.stage === 'downloading') seen.add(s.downloaded); if (s.stage === 'ready' || s.stage === 'failed') break; await sleep(40); }
check('download reports real, growing byte counts', seen.size >= 3 && s.total === payload.length, `${seen.size} progress values, total ${s.total}`);
check('download ends verified and ready', s.stage === 'ready' && s.downloaded === payload.length && stages.has('downloading'));
check('one download only, even when asked twice', hits.payload - payloadBefore === 1, `${hits.payload - payloadBefore} download`);

// 6. A development build never installs.
const refused = await invoke('install_update', { pauseReplay: true });
s = await status();
check('install is refused in a development build; nothing runs', String(refused.__rejected ?? '').includes('development build') && s.stage === 'ready');

// 7. Rejections.
const failsWith = async (name, text) => {
  use(name); let r = await invoke('check_update');
  if (r.stage === 'available') { await invoke('update_download', { install: false }); r = await until(`${name} result`, x => x.stage === 'failed' || x.stage === 'ready'); }
  return { ok: r.stage === 'failed' && (r.error ?? '').includes(text), r };
};
let f = await failsWith('tampered', 'signature is not valid');
check('a tampered download is rejected (signature)', f.ok, f.r.error ?? f.r.stage);
f = await failsWith('wrongkey', 'signature is not valid');
check('an update signed with another key is rejected', f.ok, f.r.error ?? f.r.stage);
f = await failsWith('oldsig', 'signature is not valid');
check('an old signed installer offered as a newer version is rejected', f.ok, f.r.error ?? f.r.stage);
use('older'); s = await invoke('check_update');
check('an older version is never offered (no downgrade)', s.stage === 'up_to_date' && s.latest === null);
use('same'); s = await invoke('check_update');
check('the same version is not an update', s.stage === 'up_to_date' && s.latest === null);
check('up to date: no dot, no pop-up', await js(`!document.querySelector('.nav-dot') && !document.querySelector('.update-pop')`) === true);
f = await failsWith('noarch', '64-bit Windows');
check('a release without a Windows build is reported clearly', f.ok, f.r.error ?? f.r.stage);
f = await failsWith('badjson', 'could not be read');
check('unreadable update information is reported clearly', f.ok, f.r.error ?? f.r.stage);
for (const code of ['e500', 'e404']) {
  use(code); s = await invoke('check_update');
  check(`server answer ${code.slice(1)}: failed check, not falsely up to date`, s.stage === 'failed' && !!s.error, s.error ?? s.stage);
}
f = await failsWith('interrupted', '');
check('an interrupted transfer is rejected; it never becomes ready', f.ok, f.r.error ?? f.r.stage);
use('hang'); const t0 = Date.now(); s = await invoke('check_update'); const took = (Date.now() - t0) / 1000;
check('a server that never answers ends in a timeout', s.stage === 'failed' && (s.error ?? '').includes('too long') && took > 15 && took < 40, `${took.toFixed(1)} s`);
check('failures are counted for back-off', JSON.parse(fs.readFileSync(path.join(dataDir, 'updates.json'), 'utf8')).failures >= 1);

// 8. Preferences: saved at once, persisted, and Auto mode downloads by itself.
let settings = await invoke('update_configure', { settings: { auto_check: true, interval_hours: 12, auto_download: true, auto_install: true, notify: false, idle_minutes: 15 } });
const disk = JSON.parse(fs.readFileSync(path.join(dataDir, 'settings.json'), 'utf8'));
check('update preferences are stored in settings.json', settings.updates?.auto_install === true && disk.updates?.interval_hours === 12 && disk.updates?.idle_minutes === 15 && disk.updates?.notify === false);
settings = await invoke('update_configure', { settings: { auto_check: false, interval_hours: 12, auto_download: true, auto_install: true, notify: true, idle_minutes: 15 } });
check('inconsistent switches are corrected (no auto install without checking)', settings.updates?.auto_download === false && settings.updates?.auto_install === false);
await invoke('update_configure', { settings: { auto_check: true, interval_hours: 6, auto_download: true, auto_install: true, notify: true, idle_minutes: 5 } });
use('newer'); const dl = hits.payload; await invoke('check_update');
s = await until('automatic download', x => x.stage === 'ready' || x.stage === 'failed', 20000);
check('Auto update mode downloads and verifies without a click', s.stage === 'ready' && hits.payload - dl === 1);
await sleep(1500); s = await status();
check('Auto update never installs from a development build', s.stage === 'ready' && execSync('tasklist /FI "IMAGENAME eq replayx.exe" /NH', { encoding: 'utf8' }).includes('replayx.exe'));
await invoke('update_configure', { settings: { auto_check: true, interval_hours: 6, auto_download: false, auto_install: false, notify: true, idle_minutes: 5 } });

// 9. Offline.
await new Promise(r => { server.closeAllConnections?.(); server.close(r); });
s = await invoke('check_update');
check('offline: a clear message, the app keeps working', s.stage === 'failed' && (s.error ?? '').includes("Couldn't reach GitHub"), s.error ?? s.stage);

// 10. Development preview (simulated data) through the same state machine.
s = await invoke('update_simulate', { scenario: 'available' });
check('simulation: labelled test data, pop-up offered', s.simulated === true && s.stage === 'available' && s.prompt === true && (s.detail ?? '').includes('Test data'));
await invoke('update_download', { install: true });
const simStages = new Set(); for (let i = 0; i < 300; i++) { s = await status(); simStages.add(s.stage); if (s.stage === 'idle') break; await sleep(60); }
check('simulation runs download, verify, install and ends unchanged', ['downloading', 'verifying', 'installing', 'idle'].every(x => simStages.has(x)) && (s.detail ?? '').includes('Nothing was downloaded or installed'), [...simStages].join(' > '));
s = await invoke('update_simulate', { scenario: 'invalid_signature' }); await invoke('update_download', { install: true });
s = await until('simulated bad signature', x => x.stage === 'failed', 15000);
check('simulation: a bad signature ends in the rejection message', (s.error ?? '').includes('signature is not valid'));
await invoke('update_simulate', { scenario: 'reset' });

// 11. Microphone switch (no capture is running: the microphone is never opened).
const mic = st => st.audio.find(a => a.kind === 'microphone');
settings = await invoke('settings');
const voiceBefore = JSON.stringify({ ...mic(settings), muted: undefined, enabled: undefined });
settings = await invoke('set_microphone', { on: false });
check('microphone off: muted, still set up, nothing else changed', mic(settings).muted === true && mic(settings).enabled === true && JSON.stringify({ ...mic(settings), muted: undefined, enabled: undefined }) === voiceBefore);
check('system audio is unaffected', settings.audio.filter(a => a.kind === 'system').every(a => a.enabled && !a.muted));
await sleep(400);
check('home page switch shows Off after the engine confirmed', await js(`document.querySelector('.mic-switch')?.getAttribute('aria-checked')`) === 'false' && await js(`!!document.querySelector('.mic-row.off .off-tag')`) === true);
await js(`document.querySelector('.mic-switch').click()`); await sleep(700);
settings = await invoke('settings');
check('clicking the home page switch turns the microphone on in the engine', mic(settings).muted === false && await js(`document.querySelector('.mic-switch')?.getAttribute('aria-checked')`) === 'true');
await Promise.all(Array.from({ length: 30 }, (_, i) => invoke('set_microphone', { on: i % 2 === 0 })));
await sleep(500);
settings = await invoke('settings'); const shown = await js(`document.querySelector('.mic-switch')?.getAttribute('aria-checked')`);
check('30 rapid switches: engine, saved file and switch agree', String(!mic(settings).muted) === shown && mic(JSON.parse(fs.readFileSync(path.join(dataDir, 'settings.json'), 'utf8'))).muted === mic(settings).muted, `final: ${shown === 'true' ? 'on' : 'off'}`);
await invoke('set_microphone', { on: false });
check('the choice survives a restart (saved)', mic(JSON.parse(fs.readFileSync(path.join(dataDir, 'settings.json'), 'utf8'))).muted === true);
const st = await invoke('status');
check('no capture was started by any of this', st.replay === false && st.recording === false && st.state === 'Stopped');

ws.close(); stopApp();
const failed = results.filter(r => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed${failed.length ? `; FAILED: ${failed.map(r => r.name).join(' | ')}` : ''}`);
fs.writeFileSync(path.join(work, 'results.json'), JSON.stringify({ results, passed: results.length - failed.length, failed: failed.length, actualInstallationTested: false }, null, 2));
console.log('Retained isolated evidence:', work);
process.exit(failed.length ? 1 : 0);
