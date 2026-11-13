import { useEffect, useState } from 'react';
import { ArrowDownToLine, ExternalLink, RotateCw, ShieldCheck, X } from 'lucide-react';
import { api, type UpdateStatus } from './api';
import { useStatus } from './store';
import { useUpdate } from './updates';
import { highlights, percent, progressText, stageLabel } from './update-logic';
import { Button } from './ui';

/** What installing now would interrupt, in the user's words (null = nothing). */
export function useInstallHint(): string | null {
  const c = useStatus(x => ({ recording: x.recording || x.pending_recording, saving: x.saving > 0, replay: x.replay }));
  if (c.recording) return 'A recording is in progress. The update can download now; it installs after you stop recording.';
  if (c.saving) return 'A clip is being saved. The update installs once the save has finished.';
  if (c.replay) return 'Instant Replay will pause for the update and turn back on afterwards. Replay footage you have not saved is discarded.';
  return null;
}

/** Real progress from the engine: bytes while downloading, then verifying. */
export function UpdateProgress({ u }: { u: UpdateStatus }) {
  const p = u.stage === 'downloading' ? percent(u) : null;
  const working = u.stage === 'verifying' || u.stage === 'installing' || u.stage === 'checking' || (u.stage === 'downloading' && p === null);
  return (
    <div className="up-progress" role="status" aria-live="polite">
      <div className={`up-bar ${working ? 'working' : ''}`} role="progressbar" aria-label={stageLabel(u)} aria-valuemin={0} aria-valuemax={100} aria-valuenow={p ?? undefined}>
        <i style={{ width: `${u.stage === 'downloading' ? p ?? 100 : 100}%` }} />
      </div>
      <div className="up-progress-text">
        <span>{stageLabel(u)}</span>
        {u.stage === 'downloading' && <span className="mono">{progressText(u)}</span>}
        {u.stage === 'verifying' && <span><ShieldCheck size={13} /> Checking the publisher's signature</span>}
      </div>
    </div>
  );
}

export const updateNow = () => api.updateDownload(true).catch(() => undefined);
export const installNow = () => api.installUpdate(true).catch(() => undefined);

/** The "update available" pop-up: a compact card in the top-right corner.
 * Shown when the engine says so (a newer signed release, not dismissed in the
 * last few days) and never over the Updates tab, which shows the same thing.
 * It doesn't take focus and doesn't block the app. */
export function UpdatePrompt({ suppressed }: { suppressed: boolean }) {
  const u = useUpdate();
  const hint = useInstallHint();
  // After "Update now" the card stays to show progress and the result.
  const [following, setFollowing] = useState(false);
  useEffect(() => {
    if (following && (u.stage === 'up_to_date' || (u.stage === 'idle' && !u.detail))) setFollowing(false);
  }, [following, u.stage, u.detail]);
  const offer = u.prompt && (u.stage === 'available' || u.stage === 'ready');
  if (suppressed || !(offer || following)) return null;
  const news = highlights(u.notes, 3);
  const close = () => setFollowing(false);
  const working = u.stage === 'downloading' || u.stage === 'verifying' || u.stage === 'installing';
  return (
    <aside className="update-pop" role="dialog" aria-label="REPLAYX update" aria-live="polite">
      <div className="up-glow" aria-hidden="true" />
      <header>
        <span className="up-icon" aria-hidden="true"><ArrowDownToLine size={18} /></span>
        <div>
          <h2>{u.stage === 'available' ? 'A new REPLAYX update is available' : stageLabel(u)}</h2>
          {u.latest && <p className="up-version">Version {u.latest}{u.date ? ` · ${u.date}` : ''}{u.simulated && <em className="test-tag">Test data</em>}</p>}
        </div>
        {!working && <button className="up-close" aria-label={u.stage === 'available' ? 'Not now' : 'Close'} onClick={() => { if (u.stage === 'available' || u.stage === 'ready') void api.updateDismiss(); close(); }}><X size={15} /></button>}
      </header>

      {u.stage === 'available' && (
        <>
          <p className="up-text">A newer version is ready with improvements and bug fixes.</p>
          {news.length > 0 && (
            <div className="up-news">
              <span>What's new</span>
              <ul>{news.map((n, i) => <li key={i}>{n}</li>)}</ul>
            </div>
          )}
          {hint && <p className="up-hint">{hint}</p>}
          <div className="up-actions">
            <Button variant="primary" icon={ArrowDownToLine} onClick={() => { setFollowing(true); void updateNow(); }}>Update Now</Button>
            <Button variant="ghost" onClick={() => { void api.updateDismiss(); close(); }}>Not Now</Button>
            <button className="link-btn up-notes" onClick={() => void api.updateOpenNotes().catch(() => undefined)}>View release notes <ExternalLink size={12} /></button>
          </div>
        </>
      )}

      {working && <UpdateProgress u={u} />}
      {working && u.detail && <p className="up-hint">{u.detail}</p>}

      {u.stage === 'ready' && (
        <>
          <p className="up-text">{u.detail ?? 'The update is downloaded and its signature is verified.'}</p>
          {!u.detail && hint && <p className="up-hint">{hint}</p>}
          <div className="up-actions">
            <Button variant="primary" icon={RotateCw} disabled={!u.installable && !u.simulated} onClick={() => { setFollowing(true); void installNow(); }}>Install Now</Button>
            <Button variant="ghost" onClick={() => { void api.updateDismiss(); close(); }}>Later</Button>
          </div>
        </>
      )}

      {u.stage === 'failed' && (
        <>
          <p className="up-text bad">{u.error}</p>
          <div className="up-actions">
            <Button variant="primary" icon={RotateCw} onClick={() => void (u.latest ? updateNow() : api.checkUpdate().catch(() => undefined))}>Try Again</Button>
            <Button variant="ghost" onClick={close}>Close</Button>
          </div>
        </>
      )}

      {u.stage === 'idle' && u.detail && (
        <>
          <p className="up-text">{u.detail}</p>
          <div className="up-actions"><Button variant="ghost" onClick={close}>Close</Button></div>
        </>
      )}
    </aside>
  );
}
