import { test } from 'node:test';
import assert from 'node:assert/strict';
import { diagnosticReport } from '../src/diagnostics.ts';
import type { Status, Settings } from '../src/api.ts';

test('shareable diagnostics omit paths, identities and arbitrary error strings', () => {
  const s = { state: 'Error', encoder: 'h264_nvenc', capture: 'desktop-duplication', scaling: 'GPU',
    error: 'Audio failed C:\\Users\\Private\\secret.mp4 token=private', last_clip: 'SECRET',
    detected_game: 'PRIVATE', audio_written_bytes: [1024], audio_discontinuities: [0],
    audio_drift_ppm: [2], frames: 300, fps: 60, audio_queue_ms: [20],
    scaling_detail: 'C:\\private path', output_width: 1920, output_height: 1080 } as Status;
  const r = diagnosticReport(s, { fps: 60, encoder: 'auto', output_dir: 'PRIVATE' } as Settings, '0.3.1-alpha.7');
  assert.equal(r.lastErrorCategory, 'audio');
  assert.equal(r.encodedFrames, 300);
  assert.equal(r.sourceFPS, null);
  assert.deepEqual(r.audioQueueMs, [20]);
  assert.doesNotMatch(JSON.stringify(r), /private|secret|Users|token=/i);
});

test('GPU input paths survive redaction without leaking fallback stderr', () => {
  const s = { scaling: 'GPU zero-copy (Direct3D 11 → encoder)', error: null,
    geometry: { adapter_vendor: 0x10de, desktop_width: 1920, desktop_height: 1080 },
    start_timings: [{ stage: 'Encoder GPU priority raised (high)', elapsed_ms: 1 }],
    state: 'Buffering', capture: 'desktop-duplication', encoder: 'h264_nvenc',
    scaling_detail: 'private path' } as Status;
  const r = diagnosticReport(s, { encoder: 'auto', fps: 60 } as Settings, '0.3.1-alpha.7');
  assert.equal(r.scaling, 'd3d11');
  assert.equal(r.encoderInput, 'GPU BGRA');
  assert.equal(r.captureAdapterVendor, 0x10de);
  assert.equal(r.captureGpuPriority, 'high');
  assert.doesNotMatch(JSON.stringify(r), /private/);
});

test('capture method reports what ran, not the source setting, and a fallback', () => {
  const ran = { capture: 'desktop-duplication', capture_api: 'wgc', capture_filter: 'gfxcapture', capture_fallback: null,
    scaling: 'GPU zero-copy (Windows Graphics Capture → NVENC)', state: 'Buffering', encoder: 'h264_nvenc' } as unknown as Status;
  const wgc = { encoder: 'h264_nvenc', fps: 60, capture_api: 'wgc' } as Settings;
  const r = diagnosticReport(ran, wgc, '0.3.1-alpha.8');
  assert.equal(r.captureSource, 'display');
  assert.equal(r.captureMethod, 'windows-graphics-capture');
  assert.equal(r.captureFilter, 'gfxcapture');
  assert.equal(r.captureFallback, false);
  assert.equal(r.scaling, 'wgc to nvenc');
  assert.equal(r.encoderInput, 'GPU BGRA');
  const fell = { ...ran, capture_api: 'dda', capture_filter: 'ddagrab', capture_fallback: 'Failed C:\private' } as unknown as Status;
  const f = diagnosticReport(fell, wgc, '0.3.1-alpha.8');
  assert.equal(f.captureMethod, 'desktop-duplication');
  assert.equal(f.requestedCaptureMethod, 'windows-graphics-capture');
  assert.equal(f.captureFilter, 'ddagrab');
  assert.equal(f.captureFallback, true);
  assert.doesNotMatch(JSON.stringify(f), /private/);
});
