// Development-only UI preview with mocked IPC (never part of the app build).
// Run: npx vite --config dev/vite.preview.ts   then open /dev/preview.html#home
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type { Clip, Settings, Status } from '../src/api';

mockWindows('main');
// Surface uncaught errors in the screenshot itself.
const showError = (text: string) => {
  const pre = document.createElement('pre');
  pre.style.cssText = 'position:fixed;inset:auto 0 0 0;max-height:60vh;overflow:auto;margin:0;padding:12px;background:#300;color:#fbb;font:12px monospace;z-index:99;white-space:pre-wrap';
  pre.textContent = text;
  document.body.appendChild(pre);
};
window.addEventListener('error', e => showError(`${e.message}
${e.error?.stack ?? ''}`));
window.addEventListener('unhandledrejection', e => showError(`Unhandled: ${e.reason?.stack ?? e.reason}`));
const consoleError = console.error;
console.error = (...a: unknown[]) => { showError(a.map(x => (x instanceof Error ? x.stack : String(x))).join(' ')); consoleError(...a); };
// App modules read the Tauri globals at load time, so import them after mocking.
let emptyStatus = {} as Status;
let previewSettings = {} as Settings;
const scenario = new URLSearchParams(window.location.search).get('state') ?? 'replay';
let settings = {} as Settings;
const plain = { boost_db: 0, high_pass: false, noise_suppression: false, noise_gate: false, compressor: false };
const initialSettings = (): Settings => ({
  ...previewSettings,
  output_dir: 'C:\\Users\\You\\Videos\\REPLAYX',
  audio: [
    { kind: 'application', device: '', pid: 29336, process_started: 1, name: 'Game', enabled: true, volume: 1, muted: false, ...plain },
    { kind: 'application', device: '', pid: 25872, process_started: 1, name: 'Discord', enabled: true, volume: 0.8, muted: false, ...plain },
    { kind: 'microphone', device: 'b', pid: 0, process_started: 0, name: 'Microphone', enabled: true, volume: 1.1, muted: false, boost_db: 6, high_pass: true, noise_suppression: true, noise_gate: false, compressor: false },
  ],
});
const started = Date.now();
const status = (): Status => {
  const t = (Date.now() - started) / 1000;
  const on = scenario !== 'off';
  return {
    ...emptyStatus,
    state: !on ? 'Stopped' : scenario === 'rec' ? 'Recording' : 'Buffering', replay: on, recording: scenario === 'rec',
    encoder: on ? 'h264_nvenc' : '', encoder_label: on ? 'h264_nvenc' : '', encoder_speed: on ? 1.0 : 0, video_lag_ms: on ? 140 : 0,
    target_bitrate_mbps: on ? 28 : 0, detected_game: on ? 'Roblox' : null, foreground_game: 'Roblox', recording_elapsed: scenario === 'rec' ? 94 + t : 0,
    scaling: on ? 'GPU zero-copy (Intel Quick Sync)' : '', output_width: 1920, output_height: 1080,
    buffer_seconds: on ? Math.min(60, 42 + t) : 0, buffer_bytes: on ? 71_000_000 : 0, free_bytes: 412 * 1024 ** 3,
    frames: on ? 2650 + Math.round(t * 60) : 0, dropped_frames: 0, fps: on ? 59.7 + Math.sin(t) * 0.2 : 0, elapsed: on ? 154 + t : 0,
    write_mbps: 1.6, memory_mb: 212, cpu_percent: 2.9 + Math.sin(t / 2) * 0.4, frame_age_ms: 16, segment_age_ms: 900,
    audio_peaks: on ? [0.35 + 0.2 * Math.abs(Math.sin(t * 3)), 0.12 * Math.abs(Math.sin(t * 5)), 0.05] : [],
    audio_packets: on ? [15400, 15388, 15402] : [], audio_discontinuities: on ? [0, 0, 0] : [], audio_drift_ppm: on ? [0.4, -1.2, 12.5] : [],
    audio_written_bytes: on ? [57e6, 57e6, 57e6] : [], audio_gaps: [0, 0, 0],
    geometry: { index: 0, device: '\\\\.\\DISPLAY1', adapter_luid: '0:1', left: 0, top: 0, desktop_width: 2560, desktop_height: 1600, active_width: 2560, active_height: 1600, rotation: 1, refresh_hz: 240 },
    codec: on ? 'h264' : '', constant_quality: on ? 28 : null, cap_bitrate_mbps: on ? 24 : 0,
    scaling_detail: on ? 'GPU zero-copy (Direct3D 11) + h264_nvenc: Could not create the texture (80070057)' : null,
  };
};
const svg = (hue: number, label: string) => `data:image/svg+xml;utf8,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 180"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${hue},45%,22%)"/><stop offset="1" stop-color="hsl(${hue + 40},40%,10%)"/></linearGradient></defs><rect width="320" height="180" fill="url(#g)"/><text x="16" y="164" fill="rgba(255,255,255,.5)" font-family="Segoe UI" font-size="14">${label}</text></svg>`)}`;
const probe = (d: number, titles: string[]) => ({ duration: d, bytes: d * 1.55e6, video_codec: 'h264', audio_tracks: titles.length, width: 1920, height: 1080, start_time: 0, audio_titles: titles, fps: 60 });
const clip = (category: string, name: string, kind: string, modified: number, d: number, titles: string[]): Clip =>
  ({ id: `${category}/${name}`, name, category, kind, modified, bytes: d * 3.5e6, probe: { ...probe(d, titles), game: category, kind } });
const mix = ['Playback mix', 'Game', 'Discord', 'Microphone'];
const clips: Clip[] = scenario === 'empty' ? [] : [
  clip('Roblox', 'Roblox_2026-11-07_21-30-12.mkv', 'Replay', 1794191412, 60, mix),
  clip('Fortnite', 'Fortnite_2026-11-07_20-55-44.mkv', 'Replay', 1794189344, 60, mix),
  clip('Fortnite', 'Fortnite_2026-11-07_19-30-10.mkv', 'Recording', 1794184210, 1324, mix),
  clip('Valorant', 'Valorant_2026-11-06_22-15-01.mkv', 'Replay', 1794107701, 30, mix),
  clip('Desktop', 'Desktop_2026-11-06_22-02-12.mkv', 'Replay', 1794106932, 30, ['Playback mix', 'System audio']),
];
mockIPC((cmd, args) => {
  const a = args as Record<string, unknown>;
  switch (cmd) {
    case 'status': return status();
    case 'settings': return settings;
    case 'save_settings': settings = a.settings as Settings; return null;
    case 'set_mix': return null;
    case 'capabilities': return {
      encoders: [{ name: 'h264_nvenc', available: true, reason: '' }, { name: 'h264_amf', available: false, reason: '' }, { name: 'h264_qsv', available: true, reason: '' }, { name: 'libopenh264', available: true, reason: '' }, { name: 'mpeg4', available: true, reason: '' }],
      codecs: [{ codec: 'h264', encoders: ['h264_nvenc'] }, { codec: 'hevc', encoders: ['hevc_nvenc'] }, { codec: 'av1', encoders: ['av1_nvenc'] }],
      media_error: null, updates_configured: false,
      displays: [status().geometry, { ...status().geometry!, index: 1, device: '\\\\.\\DISPLAY2', left: 2560, active_width: 1920, active_height: 1080, desktop_width: 1920, desktop_height: 1080, refresh_hz: 144 }],
      devices: [{ id: 'a', name: 'Speakers (Realtek(R) Audio)', kind: 'system' }, { id: 'b', name: 'Headset Microphone', kind: 'microphone' }],
    };
    case 'clips': return clips;
    case 'clip_thumbnail': { const i = clips.findIndex(c => c.id === a.id); return svg(230 + i * 35, clips[i]?.category ?? ''); }
    case 'audio_sessions': return [
      { pid: 29336, name: 'Game.exe', path: '', started: 1, active: true }, { pid: 25872, name: 'Discord.exe', path: '', started: 1, active: true },
      { pid: 19036, name: 'Spotify.exe', path: '', started: 1, active: false }, { pid: 24240, name: 'Browser.exe', path: '', started: 1, active: false },
    ];
    case 'app_icon': return null;
    case 'audio_test': return { seconds: 6.2, packets: 620, peak: 0.42, rms: 0.08, discontinuities: 0, device_glitches: 0, timestamp_errors: 0, drift_correction_ppm: 0, measured_device_clock_ppm: 0, endpoint_mix_format: '48000 Hz · 2 ch · float' };
    default: return null;
  }
});
({ emptyStatus, previewSettings } = await import('../src/api'));
settings = initialSettings();
await import('../src/main');
