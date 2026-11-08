/** What the Record control does for a given engine status.
 * - `cancel`: a recording was requested and capture is still starting; nothing
 *   has been recorded, so pressing it cancels (always enabled).
 * - `stop`: a recording is running (or its encoder is still warming up).
 * - `record`: no recording. */
export type RecordMode = 'record' | 'cancel' | 'stop';

export function recordMode(s: { recording: boolean; pending_recording: boolean }): RecordMode {
  if (s.pending_recording) return 'cancel';
  return s.recording ? 'stop' : 'record';
}

/** "Starting" until the encoder writes frames: the UI never says Recording early. */
export function isStarting(s: { recording: boolean; pending_recording: boolean; state: string }): boolean {
  return s.pending_recording || s.state === 'Starting';
}

/** Instant Replay switch state from the engine status (not UI state). */
export type ReplayState = 'off' | 'starting' | 'on' | 'stopping' | 'error';
export function replayState(s: { replay: boolean; state: string }): ReplayState {
  if (s.state === 'Stopping') return 'stopping';
  if (s.replay && s.state === 'Starting') return 'starting';
  if (s.replay && s.state !== 'Error') return 'on';
  return s.state === 'Error' ? 'error' : 'off';
}
