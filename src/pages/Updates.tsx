import { useState } from 'react';
import { ArrowDownToLine, CheckCircle2, ExternalLink, FlaskConical, RefreshCw, RotateCw, ShieldCheck, TriangleAlert } from 'lucide-react';
import { api, errorText, type UpdateSettings } from '../api';
import { useApp } from '../context';
import { applyUpdate, DEV, useUpdate } from '../updates';
import { applyMode, hasUpdate, highlights, IDLE_MINUTES, INTERVALS, lastChecked, modeOf, MODES, normalize, RECOMMENDED_HOURS, stageLabel } from '../update-logic';
import { installNow, updateNow, UpdateProgress, useInstallHint } from '../UpdatePrompt';
import { Badge, Button, Card, Field, Segmented, Toggle } from '../ui';

/** Settings → Updates: what the engine found, real progress, and preferences
 * that apply at once (also while capturing). */
export function UpdatesTab() {
  const { saved, syncFromEngine, toast } = useApp();
  const u = useUpdate();
  const hint = useInstallHint();
  const [checking, setChecking] = useState(false);
  const prefs = saved.updates;
  const mode = modeOf(prefs);
  const custom = prefs.interval_hours !== RECOMMENDED_HOURS;

  const configure = async (next: UpdateSettings) => {
    const wanted = normalize(next);
    syncFromEngine({ updates: wanted });
    try { syncFromEngine({ updates: (await api.updateConfigure(wanted)).updates }); }
    catch (e) { syncFromEngine({ updates: prefs }); toast('error', errorText(e)); }
  };
  const check = async () => {
    setChecking(true);
    try { applyUpdate(await api.checkUpdate()); } catch (e) { toast('error', errorText(e)); } finally { setChecking(false); }
  };
  const simulate = (scenario: string) => api.updateSimulate(scenario).then(applyUpdate).catch(e => toast('error', errorText(e)));

  const working = u.stage === 'checking' || u.stage === 'downloading' || u.stage === 'verifying' || u.stage === 'installing';
  const news = highlights(u.notes, 6);
  const tone = u.stage === 'failed' ? 'bad' : hasUpdate(u) ? 'accent' : u.stage === 'up_to_date' ? 'ok' : 'idle';
  return (
    <>
      <Card title="Updates" subtitle="New versions come from REPLAYX's GitHub Releases and are verified with the publisher's signature before they install.">
        {!u.configured && <div className="banner info">This build has no update channel configured. Download new versions from the GitHub Releases page.</div>}
        <div className={`up-status ${tone}`}>
          <span className="up-status-icon" aria-hidden="true">
            {u.stage === 'failed' ? <TriangleAlert size={20} /> : u.stage === 'up_to_date' ? <CheckCircle2 size={20} /> : <ArrowDownToLine size={20} />}
          </span>
          <div className="up-status-main">
            <strong>{stageLabel(u)}{u.simulated && <em className="test-tag">Test data</em>}</strong>
            {u.stage === 'available' && <span>A newer version is ready with improvements and bug fixes.</span>}
            {u.stage === 'up_to_date' && <span>{u.detail ?? `Version ${u.current} is the newest release.`}</span>}
            {u.stage === 'idle' && <span>{u.detail ?? 'Check to see whether a newer version has been released.'}</span>}
            {u.stage === 'failed' && <span>{u.error}</span>}
            {(u.stage === 'ready' || u.stage === 'installing') && <span>{u.detail ?? 'Downloaded, and the signature is verified.'}</span>}
          </div>
          {!working && (u.stage === 'idle' || u.stage === 'up_to_date' || (u.stage === 'failed' && !u.latest)) && (
            <Button icon={RefreshCw} disabled={!u.configured || checking} onClick={check}>Check Now</Button>
          )}
        </div>

        {working && <UpdateProgress u={u} />}

        {u.stage === 'available' && news.length > 0 && (
          <div className="up-news wide">
            <span>What's new in {u.latest}</span>
            <ul>{news.map((n, i) => <li key={i}>{n}</li>)}</ul>
          </div>
        )}
        {(u.stage === 'available' || u.stage === 'ready') && hint && !u.detail && <p className="up-hint">{hint}</p>}
        {u.stage === 'available' && (
          <div className="row">
            <Button variant="primary" icon={ArrowDownToLine} onClick={() => void updateNow()}>Update Now</Button>
            <Button onClick={() => void api.updateDownload(false).catch(() => undefined)}>Download Only</Button>
            <Button variant="ghost" icon={ExternalLink} onClick={() => void api.updateOpenNotes().catch(e => toast('error', errorText(e)))}>View release notes</Button>
            <Button variant="ghost" icon={RefreshCw} disabled={checking} onClick={check}>Check Again</Button>
          </div>
        )}
        {u.stage === 'ready' && (
          <div className="row">
            <Button variant="primary" icon={RotateCw} disabled={!u.installable && !u.simulated} onClick={() => void installNow()}>Install Now</Button>
            <Button variant="ghost" icon={ExternalLink} onClick={() => void api.updateOpenNotes().catch(e => toast('error', errorText(e)))}>View release notes</Button>
            <span className="muted small">Or keep working: it stays ready until you install it.</span>
          </div>
        )}
        {u.stage === 'failed' && u.latest && (
          <div className="row">
            <Button variant="primary" icon={RotateCw} onClick={() => void updateNow()}>Try Again</Button>
            <Button variant="ghost" icon={RefreshCw} disabled={checking} onClick={check}>Check Again</Button>
          </div>
        )}

        <dl className="facts up-facts">
          <div><dt>Current version</dt><dd>{u.current}</dd></div>
          <div><dt>Latest available</dt><dd>{u.latest ?? (u.stage === 'up_to_date' ? `${u.current} (you have it)` : 'Not checked yet')}</dd></div>
          <div><dt>Last checked</dt><dd>{lastChecked(u.last_checked)}</dd></div>
        </dl>
        <p className="muted small"><ShieldCheck size={13} className="inline-icon" /> An update only installs if its signature matches REPLAYX's publisher key. Your clips, settings and playtime are kept. A recording or a save is never interrupted.</p>
      </Card>

      <Card title="Update preferences" subtitle="These apply right away.">
        <div className="preset-cards" role="radiogroup" aria-label="Update mode">
          {MODES.map(m => (
            <button key={m.id} role="radio" aria-checked={mode === m.id} className={`preset-card ${mode === m.id ? 'on' : ''}`} onClick={() => void configure(applyMode(prefs, m.id))}>
              <strong>{m.label}{m.recommended && <em className="rec-tag">Recommended</em>}</strong>
              <small>{m.text}</small>
            </button>
          ))}
        </div>
        <Toggle label="Automatically check for updates" checked={prefs.auto_check} onChange={v => void configure({ ...prefs, auto_check: v })}
          description="Shortly after REPLAYX starts and then on a schedule. A small release listing and, when available, a signed-update manifest from GitHub; nothing about you, your games or your recordings is sent." />
        <Field label="Check frequency">
          <div className="row">
            <Segmented label="Check frequency" value={custom ? 'custom' : 'recommended'} disabled={!prefs.auto_check}
              onChange={v => void configure({ ...prefs, interval_hours: v === 'recommended' ? RECOMMENDED_HOURS : prefs.interval_hours === RECOMMENDED_HOURS ? 24 : prefs.interval_hours })}
              options={[{ value: 'recommended', label: `Recommended (every ${RECOMMENDED_HOURS} hours)` }, { value: 'custom', label: 'Custom' }]} />
            {custom && (
              <select aria-label="Hours between checks" value={prefs.interval_hours} disabled={!prefs.auto_check} onChange={e => void configure({ ...prefs, interval_hours: Number(e.target.value) })}>
                {[...new Set([...INTERVALS, prefs.interval_hours])].sort((a, b) => a - b).map(h => <option key={h} value={h}>Every {h === 1 ? 'hour' : `${h} hours`}</option>)}
              </select>
            )}
          </div>
        </Field>
        <Toggle label="Notify me when updates are available" checked={prefs.notify} onChange={v => void configure({ ...prefs, notify: v })}
          description="A pop-up in REPLAYX, or a small notice when it is in the tray. The dot on Settings shows either way." />
        <Toggle label="Automatically download updates" checked={prefs.auto_download} disabled={!prefs.auto_check} onChange={v => void configure({ ...prefs, auto_download: v })}
          description="Downloads in the background and verifies the signature. Nothing installs until you say so, unless the next switch is on." />
        <Toggle label="Automatically install updates" checked={prefs.auto_install} disabled={!prefs.auto_download} onChange={v => void configure({ ...prefs, auto_install: v })}
          description="Installs and restarts REPLAYX by itself, only after recording, Instant Replay and saves have stopped, no game is in front and you have been away from the PC." />
        {prefs.auto_install && (
          <Field label="Install after the PC has been idle for">
            <Segmented label="Idle time before an automatic install" value={prefs.idle_minutes} onChange={v => void configure({ ...prefs, idle_minutes: v })}
              options={[...new Set([...IDLE_MINUTES, prefs.idle_minutes])].sort((a, b) => a - b).map(m => ({ value: m, label: `${m} min` }))} />
          </Field>
        )}
      </Card>

      {DEV && (
        <Card title="Update preview" subtitle="Development build only.">
          <div className="banner info"><FlaskConical size={16} />Test data. These buttons show the update screens without contacting GitHub; nothing is downloaded or installed, and the installed REPLAYX is never touched.</div>
          <div className="row">
            <Button onClick={() => void simulate('available')}>Simulate: update available</Button>
            <Button onClick={() => void simulate('invalid_signature')}>Simulate: bad signature</Button>
            <Button onClick={() => void simulate('up_to_date')}>Simulate: up to date</Button>
            <Button onClick={() => void simulate('failed')}>Simulate: no connection</Button>
            <Button variant="ghost" onClick={() => void simulate('reset')}>Reset</Button>
          </div>
          {u.simulated && <p className="muted small"><Badge tone="warn">Test data</Badge> The status above is simulated.</p>}
        </Card>
      )}
    </>
  );
}
