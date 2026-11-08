import { useCapture } from './capture';
import { native } from './api';
import { replayState } from './record';
import { useStatus } from './store';

const TEXT = { off: 'Off', starting: 'Starting', on: 'On', stopping: 'Stopping', error: 'Error' } as const;

/** Instant Replay on/off, driven by the engine's real state (never local
 * React state): On only once buffering is healthy, Starting/Stopping while
 * the engine works, Error after a failed start. Turning it off never stops a
 * separate recording. */
export function ReplaySwitch({ size = 'md', label = true }: { size?: 'md' | 'lg'; label?: boolean }) {
  const { go, busy } = useCapture();
  const s = useStatus(x => ({ replay: x.replay, recording: x.recording, state: x.state, buffer: x.buffer_seconds }));
  const state = replayState(s);
  const on = state === 'on' || state === 'starting';
  const working = state === 'starting' || state === 'stopping';
  const title = state === 'on'
    ? `Instant Replay is buffering (${Math.round(s.buffer)} s kept). Click to turn off${s.recording ? '; the recording continues' : ''}.`
    : state === 'error' ? 'Capture stopped with an error (details on Home). Click to try again.'
    : working ? 'Instant Replay is changing state…' : 'Instant Replay is off. Click to start buffering.';
  return (
    <button type="button" role="switch" aria-checked={on} aria-busy={working} title={title}
      disabled={!native || working || (!!busy && !working)}
      className={`replay-switch ${size} ${state}`}
      onClick={() => go(on ? 'stop_replay' : 'start_replay')}>
      {label && <span className="replay-switch-label">Instant Replay</span>}
      <span className="replay-switch-track" aria-hidden="true">
        <span className="replay-switch-state">{TEXT[state]}</span>
        <span className="replay-switch-knob" />
      </span>
    </button>
  );
}
