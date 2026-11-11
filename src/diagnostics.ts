import type { Settings, Status } from './api';

/** An allowlist, not a dump of settings/status: paths, titles, device IDs,
 * arbitrary encoder stderr and user-entered names never leave this function. */
export function diagnosticReport(s: Status, settings: Settings, version: string) {
  const token = (v: string) => /^[a-zA-Z0-9_. -]{1,64}$/.test(v) ? v : 'unavailable';
  const number = (v: number) => Number.isFinite(v) ? v : null;
  const values = (v: number[] = []) => v.map(number);
  const error = s.error?.toLowerCase() ?? '';
  const paths: Record<string, { path: string; input: string }> = {
    'GPU zero-copy (Direct3D 11 → encoder)': { path: 'd3d11', input: 'GPU BGRA' },
    'GPU zero-copy (Intel Quick Sync)': { path: 'qsv', input: 'GPU NV12' },
    'GPU scaling (CUDA) + one frame transfer': { path: 'cuda with CPU transfer', input: 'GPU BGRA' },
    'GPU scaling (Intel VPP) + scaled-frame transfer': { path: 'qsv with CPU transfer', input: 'system NV12' },
    'CPU fallback': { path: 'cpu', input: 'system YUV420P' },
  };
  const path = paths[s.scaling];
  const errorCategory = !error ? null : error.includes('audio') ? 'audio' : error.includes('space') || error.includes('storage') ? 'storage' : error.includes('display') || error.includes('geometry') ? 'display' : error.includes('encoder') || error.includes('nvenc') ? 'encoder' : 'capture';
  return {
    version: token(version), state: token(s.state), recording: s.recording, replay: s.replay,
    backend: token(s.capture), encoder: token(s.encoder), requestedEncoder: token(settings.encoder),
    fallbackReported: !!s.scaling_detail, scaling: path?.path ?? 'unavailable',
    encoderInput: path?.input ?? 'unavailable',
    captureAdapterVendor: typeof s.geometry?.adapter_vendor === 'number' ? number(s.geometry.adapter_vendor) : null,
    desktopPixels: s.geometry ? [s.geometry.desktop_width, s.geometry.desktop_height].map(number) : null,
    captureGpuPriority: s.start_timings?.some(t => t.stage === 'Encoder GPU priority raised (high)') ? 'high'
      : s.start_timings?.some(t => t.stage === 'Encoder GPU priority raised (above normal)') ? 'above_normal' : 'unconfirmed',
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
