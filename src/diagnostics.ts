import type { Settings, Status } from './api';

/** An allowlist, not a dump of settings/status: paths, titles, device IDs,
 * arbitrary encoder stderr and user-entered names never leave this function. */
export function diagnosticReport(s: Status, settings: Settings, version: string) {
  const token = (v: string) => /^[a-zA-Z0-9_. -]{1,64}$/.test(v) ? v : 'unavailable';
  const number = (v: number) => Number.isFinite(v) ? v : null;
  const values = (v: number[] = []) => v.map(number);
  const error = s.error?.toLowerCase() ?? '';
  const errorCategory = !error ? null : error.includes('audio') ? 'audio' : error.includes('space') || error.includes('storage') ? 'storage' : error.includes('display') || error.includes('geometry') ? 'display' : error.includes('encoder') || error.includes('nvenc') ? 'encoder' : 'capture';
  return {
    version: token(version), state: token(s.state), recording: s.recording, replay: s.replay,
    backend: token(s.capture), encoder: token(s.encoder), requestedEncoder: token(settings.encoder),
    fallbackReported: !!s.scaling_detail, scaling: token(s.scaling),
    canvas: [s.output_width, s.output_height].map(number), configuredFPS: number(settings.fps),
    encodedFPS: number(s.fps), encodedFrames: number(s.frames),
    outputGridDrops: number(s.dropped_frames), outputGridDuplicates: number(s.duplicated_frames),
    sourceFPS: null, sourceDrops: null, videoQueueDepth: null, gpuUsage: null, perFrameLatency: null,
    encoderSpeed: number(s.encoder_speed), videoLagMs: number(s.video_lag_ms),
    frameAgeMs: number(s.frame_age_ms), segmentAgeMs: number(s.segment_age_ms),
    cpuPercent: number(s.cpu_percent), residentMB: number(s.memory_mb),
    bufferBytes: number(s.buffer_bytes), bufferSeconds: number(s.buffer_seconds),
    audioQueueMs: values(s.audio_queue_ms), audioBytesWritten: values(s.audio_written_bytes),
    audioDiscontinuities: values(s.audio_discontinuities), audioDriftPPM: values(s.audio_drift_ppm),
    lastErrorCategory: errorCategory,
    measurementNotes: 'FPS is encoder output, not unique captured pictures. Null means unmeasured. Error text, paths and identifiers are omitted.',
  };
}
