// Browser-only regression and comparison. Uses an isolated Chrome profile and fixture IPC.
// Build dist and dist-preview for both worktrees before running.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdtemp } from 'node:fs/promises';
import { extname, resolve, sep } from 'node:path';

const wait = ms => new Promise(resolveWait => setTimeout(resolveWait, ms));
const output = await mkdtemp(resolve('test-output/redesign-review-'));
const root = resolve('.');
const base = resolve(process.argv[2] ?? 'test-output/qualified-base');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png' };
const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost');
  const [, variant, kind, ...parts] = url.pathname.split('/');
  const folder = resolve(variant === 'base' ? base : root, kind === 'fixture' ? 'dist-preview' : 'dist');
  const file = resolve(folder, parts.join('/') || 'index.html');
  if (!file.startsWith(folder + sep)) { res.writeHead(400).end(); return; }
  try {
    // Absolute Vite asset paths use the referrer's variant and fixture.
    const raw = await readFile(file);
    const prefix = '/' + variant + '/' + kind;
    const body = extname(file) === '.html' ? raw.toString().replaceAll('"/assets/', '"' + prefix + '/assets/') : raw;
    if (extname(file) === '.js' && req.headers.referer?.includes('?boot=1')) await wait(700);
    res.writeHead(200, { 'Content-Type': types[extname(file)] ?? 'application/octet-stream', 'Cache-Control': 'public, max-age=60' }).end(body);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise(done => server.listen(0, '127.0.0.1', done));
const origin = 'http://127.0.0.1:' + server.address().port;
const readServer = server.listeners('request')[0];
server.removeListener('request', readServer);
server.on('request', (req, res) => {
  if (req.url.startsWith('/assets/') || req.url.startsWith('/proxy/')) {
    const from = new URL(req.headers.referer ?? origin, origin);
    const prefix = from.pathname.split('/').slice(0, 3).join('/');
    req.url = prefix + req.url;
    if (from.searchParams.has('boot') && req.url.includes('.js')) req.url += '?slow';
  }
  void readServer(req, res);
});

const chrome = spawn(process.env.REPLAYX_CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe', [
  '--headless=new', '--no-first-run', '--no-default-browser-check', '--disable-sync', '--disable-background-networking',
  '--disable-extensions', '--remote-debugging-pipe', '--user-data-dir=' + resolve(output, 'chrome-profile'),
], { stdio: ['ignore', 'ignore', 'ignore', 'pipe', 'pipe'], windowsHide: true });
const requests = new Map();
const problems = [];
let id = 0;
let wire = '';
chrome.stdio[4].setEncoding('utf8').on('data', data => {
  wire += data;
  for (let end = wire.indexOf('\0'); end !== -1; end = wire.indexOf('\0')) {
    const packet = JSON.parse(wire.slice(0, end));
    wire = wire.slice(end + 1);
    if (packet.method === 'Runtime.exceptionThrown') problems.push(packet.params.exceptionDetails.exception?.description ?? packet.params.exceptionDetails.text);
    if (!requests.has(packet.id)) continue;
    const { resolveRequest, reject, timer } = requests.get(packet.id);
    requests.delete(packet.id);
    clearTimeout(timer);
    if (packet.error) reject(new Error(packet.error.message));
    else resolveRequest(packet.result);
  }
});
function send(method, params = {}, sessionId) {
  return new Promise((resolveRequest, reject) => {
    const current = ++id;
    const timer = setTimeout(() => {
      requests.delete(current);
      reject(new Error('CDP timed out: ' + method));
    }, 10000);
    requests.set(current, { resolveRequest, reject, timer });
    chrome.stdio[3].write(JSON.stringify({ id: current, method, params, sessionId }) + '\0');
  });
}

async function target(context) {
  const { targetId } = await send('Target.createTarget', { url: 'about:blank', browserContextId: context });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const talk = (method, params) => send(method, params, sessionId);
  await talk('Page.enable');
  await talk('Runtime.enable');
  await talk('Performance.enable');
  await talk('Page.bringToFront');
  await talk('Emulation.setFocusEmulationEnabled', { enabled: true });
  const evaluate = async expression => {
    const result = await talk('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    return result.result.value;
  };
  const until = async expression => {
    const limit = Date.now() + 7000;
    while (Date.now() < limit) {
      if (await evaluate(expression).catch(() => false)) return;
      await wait(35);
    }
    throw new Error('Waited for ' + expression + ' at ' + await evaluate('location.href'));
  };
  const view = (width = 1280, height = 820, deviceScaleFactor = 1) =>
    talk('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor, mobile: false });
  await view();
  return { talk, evaluate, until, view, close: () => send('Target.closeTarget', { targetId }) };
}

const results = { screenshots: [], checks: [], comparison: [] };
const check = (name, condition) => { assert.ok(condition, name); results.checks.push(name); };
const selector = value => JSON.stringify(value);
const isRoute = name => 'document.querySelector(".workspace-content")?.dataset.page === ' + selector(name);
const isTour = title => 'document.querySelector("dialog[open] #tour-title")?.textContent === ' + selector(title);
const buttonClick = (name, scope = '') => 'Array.from(document.querySelectorAll(' + selector(scope + ' button') + ')).find(b => b.textContent.trim() === ' + selector(name) + ')?.click()';
const link = name => 'document.querySelector(\'[data-tour="nav-' + name + '"]\')?.click()';

async function open(tab, url, condition = 'Boolean(document.querySelector(".shell"))') {
  await tab.talk('Page.navigate', { url });
  await tab.until('location.href === ' + selector(url) + ' && (' + condition + ')');
}
async function shot(tab, name) {
  const { data } = await tab.talk('Page.captureScreenshot', { format: 'png', fromSurface: true });
  await writeFile(resolve(output, name + '.png'), Buffer.from(data, 'base64'));
  results.screenshots.push(name + '.png');
}
const fixture = (v, query, hash = 'home') => origin + '/' + v + '/fixture/dev/preview.html?' + query + '#' + hash;

try {
  const tab = await target();
  await open(tab, origin + '/new/build/index.html?boot=1', '!!document.querySelector(".boot-mark")');
  await wait(265);
  await shot(tab, '01-startup');
  await tab.until('Boolean(window.__replayxReady) && !document.getElementById("splash")');

  for (const [page, name] of Object.entries({
    home: '02-overview', replay: '03-instant-replay', capture: '04-recording', clips: '05-clips',
    games: '06-games', crosshair: '07-crosshair-studio', settings: '08-settings', audio: '13-audio', performance: '14-performance',
  })) {
    await open(tab, fixture('new', page === 'home' || page === 'performance' ? 'state=replay' : 'state=off', page), isRoute(page));
    await wait(350);
    check('loads ' + page, await tab.evaluate('document.querySelector("h1")?.textContent && !document.querySelector("body > pre")'));
    check(page + " content fits the viewport", await tab.evaluate("document.querySelector('.workspace-content').scrollWidth <= document.querySelector('.workspace-content').clientWidth + 1"));
    await shot(tab, name);
  }

  await open(tab, fixture('new', 'state=off'), isRoute('home'));
  await tab.evaluate(link('replay'));
  await tab.until(isRoute('replay'));
  await tab.evaluate(link('capture'));
  await tab.until(isRoute('capture'));
  await tab.evaluate('history.back()');
  await tab.until(isRoute('replay'));
  await tab.evaluate('history.forward()');
  await tab.until(isRoute('capture'));
  check('workspace navigation and history', true);
  await tab.evaluate(link('audio'));
  await tab.evaluate('document.querySelector(".transport [role=switch]").click()');
  await tab.until('document.querySelector(".transport [role=switch]").getAttribute("aria-checked") === "true"');
  await tab.evaluate(buttonClick('Save replay', '.transport'));
  await tab.until('document.querySelector(".toasts")?.textContent.includes("Replay saved")');
  await tab.evaluate('document.querySelector(".transport [role=switch]").click()');
  await tab.until('document.querySelector(".transport [role=switch]").getAttribute("aria-checked") === "false"');
  await tab.evaluate(buttonClick('Record', '.transport'));
  await tab.until('document.querySelector(".transport .btn-record").textContent.includes("Stop")');
  await tab.evaluate(buttonClick('Stop', '.transport'));
  await tab.until('document.querySelector(".transport .btn-record").textContent.trim() === "Record"');
  check('global Capture controls dispatch recording, replay and save IPC', true);

  await open(tab, fixture('new', 'state=off'), isRoute('home'));
  await tab.until('!!document.querySelector(".recent-item")');
  await tab.evaluate('document.querySelector(".recent-item").click()');
  await tab.until('document.querySelector("[aria-label=\'Clip details\'] h2")?.textContent.includes("Roblox")');
  await tab.evaluate(buttonClick('Play', '.detail'));
  await tab.until('document.documentElement.dataset.previewClipAction?.startsWith("play:Roblox/")');
  check('opens and plays the specific clip selected on Overview', true);

  await open(tab, fixture('new', 'state=off', 'replay'), isRoute('replay'));
  await tab.evaluate('document.querySelector(".length-slider").focus()');
  await tab.talk('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'End', code: 'End', windowsVirtualKeyCode: 35 });
  await tab.until('document.querySelector(".length-slider").getAttribute("aria-valuetext") === "30 min"');
  await tab.evaluate(buttonClick('Save changes', '.savebar'));
  await tab.until('!document.querySelector(".savebar")');
  await tab.evaluate(link('capture'));
  await tab.until(isRoute('capture'));
  await tab.evaluate(link('replay'));
  await tab.until('document.querySelector(".length-slider")?.getAttribute("aria-valuetext") === "30 min"');
  check('30-minute replay setting saves and survives navigation', true);

  await tab.evaluate(buttonClick('Studio', '.workspace-nav'));
  await tab.until(isRoute('crosshair'));
  await tab.evaluate('document.querySelector(".xh-enable [role=switch]").click()');
  await tab.until('document.querySelector(".xh-enable [role=switch]").getAttribute("aria-checked") === "true"');
  await tab.evaluate(buttonClick('Library', '.workspace-nav'));
  await tab.until(isRoute('clips'));
  await tab.evaluate(buttonClick('Studio', '.workspace-nav'));
  await tab.until('document.querySelector(".xh-enable [role=switch]")?.getAttribute("aria-checked") === "true"');
  check('Crosshair enable persists across page changes via the existing IPC', true);

  await tab.evaluate(`["Capture", "Settings", "Library", "Studio", "Capture"].forEach(title => {
    [...document.querySelectorAll(".workspace-nav button")].find(b => b.textContent.trim() === title).click();
  })`);
  await tab.until(isRoute('home'));
  check('rapid repeated navigation ends on the last selected workspace', true);

  await open(tab, fixture('new', 'state=off', 'settings:hotkeys'), isRoute('settings'));
  check('hotkeys deep link', await tab.evaluate('document.querySelector("[role=tab][aria-selected=true]")?.textContent.trim() === "Hotkeys"'));
  await tab.evaluate('document.querySelector("#preferences-tab-hotkeys").focus()');
  await tab.talk('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'ArrowRight', code: 'ArrowRight', windowsVirtualKeyCode: 39 });
  await tab.until('document.activeElement.id === "preferences-tab-updates" && location.hash === "#settings:updates"');
  check('settings tabs use keyboard navigation and move focus', true);
  await tab.evaluate('document.querySelector("#preferences-tab-hotkeys").click()');
  await tab.view(700, 570, 1.5);
  await wait(250);
  check('high-DPI shell has no horizontal overflow', await tab.evaluate('document.querySelector(".shell").scrollWidth <= innerWidth && ["record","primary"].every(kind => { const label = document.querySelector(".transport .btn-" + kind + " > span"); const box = label.getBoundingClientRect(); return box.width > 0 && box.right <= innerWidth; })'));
  await shot(tab, '15-compact-hotkeys');
  await tab.view();

  for (const [state, extra, screenshot, expected] of [
    ['empty', '', '16-empty-clips', 'Your library is empty'],
    ['error', '&settings=damaged', '17-recoverable-errors', 'System audio'],
  ]) {
    await open(tab, fixture('new', 'state=' + state + extra, state === 'empty' ? 'clips' : 'home'));
    const wanted = 'document.body.textContent.includes(' + selector(expected) + ')';
    await tab.until(wanted);
    await wait(260);
    await shot(tab, screenshot);
    check('renders ' + state, true);
  }

  const clearTour = () => tab.evaluate('localStorage.removeItem("replayx.tour.v1")');
  await clearTour();
  const welcome = fixture('new', 'state=off&onboarding=fresh');
  await open(tab, welcome, isTour('Welcome to REPLAYX'));
  await wait(240);
  await shot(tab, '09-welcome');
  await tab.evaluate(buttonClick('Start tour', 'dialog'));
  await tab.until(isTour('Workspaces'));
  await tab.evaluate(buttonClick('Next', 'dialog'));
  await tab.until(isTour('Record'));
  await wait(250);
  await shot(tab, '10-guided-tour');
  await tab.evaluate(buttonClick('Back', 'dialog'));
  await tab.until(isTour('Workspaces'));
  await tab.talk('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'ArrowRight', code: 'ArrowRight', windowsVirtualKeyCode: 39 });
  await tab.until(isTour('Record'));
  await tab.talk('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  await tab.until('!document.querySelector("dialog[open]")');
  check('tour Back, ArrowRight, Escape and persistent dismissal', await tab.evaluate('JSON.parse(localStorage.getItem("replayx.tour.v1")).outcomes.welcome === "dismissed"'));
  await open(tab, welcome);
  await wait(300);
  check('dismissed welcome stays closed on relaunch', await tab.evaluate('!document.querySelector("dialog[open]")'));
  await tab.evaluate('document.querySelector(".workspace-help").click()');
  await tab.until(isTour('Welcome to REPLAYX'));
  await tab.evaluate(buttonClick('Start tour', 'dialog'));
  for (let step = 2; step <= 12; ++step) {
    await tab.until('document.querySelector(".tour-progress")?.textContent === "Step ' + (step - 1) + ' of 12"');
    await tab.evaluate(buttonClick('Next', 'dialog'));
  }
  await tab.until(isTour('Settings and shortcuts'));
  await tab.evaluate(buttonClick('Finish', 'dialog'));
  await tab.until('!document.querySelector("dialog[open]")');
  check('all twelve steps plus manual replay and persisted finish', await tab.evaluate('JSON.parse(localStorage.getItem("replayx.tour.v1")).outcomes.welcome === "completed"'));

  await clearTour();
  await open(tab, fixture('new', 'state=rec&onboarding=fresh'));
  await wait(300);
  check('welcome deferred while recording', await tab.evaluate('!document.querySelector("dialog[open]")'));
  await open(tab, fixture('new', 'state=off&onboarding=fresh&game'));
  await wait(300);
  check('welcome deferred with a foreground game', await tab.evaluate('!document.querySelector("dialog[open]")'));
  await clearTour();
  await open(tab, fixture('new', 'state=off&onboarding=fresh&settings=damaged'));
  await wait(300);
  check('welcome deferred when settings are damaged', await tab.evaluate('!document.querySelector("dialog[open]")'));

  await clearTour();
  await tab.talk('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
  await open(tab, fixture('new', 'state=off&onboarding=features'), isTour("What's new in 0.4.0"));
  check('upgrade shows exactly three features and no animations in reduced motion', await tab.evaluate('document.querySelectorAll(".release-features li").length === 3 && document.getAnimations().length === 0'));
  await shot(tab, '11-whats-new');
  await tab.evaluate(buttonClick('Show me what’s new', 'dialog'));
  await tab.until(isTour('A new capture workspace'));
  // Force the second real feature anchor to be absent before its page is opened.
  await tab.evaluate(`window.fixtureMissing = new MutationObserver(() => {
    const node = document.querySelector('[data-tour="crosshair-studio"]');
    if (node) node.removeAttribute('data-tour');
  }); window.fixtureMissing.observe(document.getElementById('root'), { childList: true, subtree: true });`);
  await tab.evaluate(buttonClick('Next', 'dialog'));
  await tab.until(isTour('Crosshair Studio'));
  await tab.until('document.querySelector(".tour-tip")?.textContent.includes("currently unavailable")');
  await tab.evaluate('window.fixtureMissing.disconnect()');
  await tab.evaluate(buttonClick('Next', 'dialog'));
  await tab.until(isTour('Experimental Native Engine'));
  check('feature-only tour continues when a spotlight anchor is missing', true);

  await clearTour();
  await open(tab, fixture('new', 'state=off&onboarding=fixes&failedNotes'), isTour('REPLAYX Updated'));
  check('fix-only update gives notes without guided steps', await tab.evaluate('!document.querySelector(".release-features") && !!document.querySelector(".release-fixes")'));
  await shot(tab, '12-fix-only-sample');
  await tab.evaluate(buttonClick('Full release notes on GitHub', 'dialog'));
  await tab.until('document.querySelector(".release-note-error")?.textContent.includes("Could not open release notes")');
  await tab.evaluate(buttonClick('All releases', 'dialog'));
  await tab.evaluate(buttonClick('Done', 'dialog'));
  await tab.until('!document.querySelector("dialog[open]")');
  check('release notes failure exposes feedback and a release-list fallback', true);
  await tab.talk('Emulation.setEmulatedMedia', { features: [] });
  await open(tab, fixture('new', 'state=off&update=verifying', 'settings:updates'), isRoute('settings'));
  await tab.until('document.getAnimations().some(a => a.effect?.getTiming().iterations === Infinity)');
  await tab.evaluate(`Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' });
    document.dispatchEvent(new Event('visibilitychange'));`);
  await tab.until('document.documentElement.hasAttribute("data-ui-paused") && document.getAnimations().every(a => a.playState !== "running")');
  check('hiding the app pauses active CSS animations without a JS render loop', true);
  await tab.close();

  if (!process.argv.includes('--no-benchmark')) {
    for (let pair = 0; pair < 4; ++pair) {
      for (const variant of (pair % 2 === 0 ? ['base', 'new'] : ['new', 'base'])) {
        const { browserContextId } = await send('Target.createBrowserContext');
        const sample = await target(browserContextId);
        const file = origin + '/' + variant + '/build/index.html';
        await open(sample, file, 'Boolean(window.__replayxReady && document.querySelector(".shell"))');
        const cold = await sample.evaluate('window.__replayxReady');
        const again = file + '?warm=' + pair;
        await open(sample, again, 'Boolean(window.__replayxReady && document.querySelector(".shell"))');
        const warm = await sample.evaluate('window.__replayxReady');
        await open(sample, fixture(variant, 'state=off'));
        await wait(300);
        const metrics = async () => Object.fromEntries((await sample.talk('Performance.getMetrics')).metrics.map(({ name, value }) => [name, value]));
        const before = await metrics();
        const processesBefore = new Map((await send('SystemInfo.getProcessInfo')).processInfo.map(p => [p.id, p.cpuTime]));
        await wait(2000);
        const after = await metrics();
        const continuousAnimations = await sample.evaluate('document.getAnimations().filter(a => a.effect?.getTiming().iterations === Infinity).length');
        if (variant === 'new') assert.equal(continuousAnimations, 0, 'Off-screen thumbnail placeholders must never animate.');
        const processes = (await send('SystemInfo.getProcessInfo')).processInfo;
        const cpuMs = type => 1000 * processes.filter(p => p.type === type && processesBefore.has(p.id))
          .reduce((total, p) => total + Math.max(0, p.cpuTime - processesBefore.get(p.id)), 0);
        const navigation = await sample.evaluate(`new Promise(done => {
          const start = performance.now(), times = [];
          ${link('capture')};
          const frame = t => {
            times.push(t);
            if (times.length < 15) requestAnimationFrame(frame);
            else done({ twoFramesMs: times[1] - start, longestFrameMs: Math.max(...times.slice(1).map((time, i) => time - times[i])) });
          };
          requestAnimationFrame(frame);
        })`);
        const pids = processes.map(p => p.id).filter(Number.isSafeInteger);
        const workingSet = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
          '(Get-Process -Id ' + pids.join(',') + ' -ErrorAction SilentlyContinue | Measure-Object WorkingSet64 -Sum).Sum'],
          { encoding: 'utf8', windowsHide: true });
        const chromeWorkingSetMiB = workingSet.status === 0 ? Number(workingSet.stdout.trim()) / 1048576 : null;
        await sample.view(960, 425, 1);
        await open(sample, fixture(variant, 'state=off&perf=scroll', 'clips'));
        await sample.until('!!document.querySelector(".clip-card")');
        const scrolling = await sample.evaluate(`new Promise(done => {
          const content = document.querySelector('.content');
          const start = performance.now();
          content.scrollTop = content.scrollHeight;
          requestAnimationFrame(() => requestAnimationFrame(() => done({
            scrollMs: performance.now() - start, scrollPixels: content.scrollTop
          })));
        })`);
        await open(sample, fixture(variant, 'state=off&perf=prefs', 'settings'));
        const settings = await sample.evaluate(`new Promise(done => {
          const start = performance.now();
          [...document.querySelectorAll('[role=tab]')].find(tab => tab.textContent.trim() === 'Hotkeys').click();
          requestAnimationFrame(() => requestAnimationFrame(() => done({
            settingsMs: performance.now() - start,
            hotkeysSelected: document.querySelector('[role=tab][aria-selected=true]')?.textContent.trim() === 'Hotkeys',
          })));
        })`);
        assert.ok(scrolling.scrollPixels > 0 && settings.hotkeysSelected, 'Expected real library scroll and settings interaction');
        results.comparison.push({ pair, variant, cold, warm, idleTaskMs: (after.TaskDuration - before.TaskDuration) * 1000, continuousAnimations, idleRendererCpuMs: cpuMs("renderer"), idleGpuProcessCpuMs: cpuMs("GPU"), heapMiB: after.JSHeapUsedSize / 1048576, nodes: after.Nodes, chromeWorkingSetMiB, ...navigation, ...scrolling, ...settings });
        await sample.close();
        await send('Target.disposeBrowserContext', { browserContextId });
      }
    }
  }
  check('no uncaught browser exceptions', problems.length === 0);
  console.log('Browser checks passed:', results.checks.length);
} finally {
  await writeFile(resolve(output, 'results.json'), JSON.stringify({ ...results, problems }, null, 2) + '\n');
  console.log('Evidence:', output);
  await send('Browser.close').catch(() => undefined);
  chrome.kill();
  server.close();
}
