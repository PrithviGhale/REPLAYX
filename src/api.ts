import { invoke } from '@tauri-apps/api/core';

/** True inside the Tauri window; false in a plain browser (UI preview only). */
export const native = !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;

export type AudioKind = 'system' | 'microphone' | 'application' | 'exclude';
export type AudioSource = {
  kind: AudioKind; device: string; pid: number; process_started: number;
  name: string; enabled: boolean; volume: number; muted: boolean;
  boost_db: number; high_pass: boolean; noise_suppression: boolean; noise_gate: boolean; compressor: boolean;
};
export type Rect = { x: number; y: number; width: number; height: number };
export type Display = {
  index: number; device: string; adapter_luid: string; active_width: number; active_height: number;
  desktop_width: number; desktop_height: number; refresh_hz: number; rotation: number; left: number; top: number;
};
export type CanvasMode = 'stretch' | 'fit' | 'crop' | 'native';
export type Settings = {
  capture: string; display: number; window_title: string; width: number; height: number; canvas_mode: CanvasMode;
  source_rect: Rect | null; fps: number; encoder: string; codec: 'auto' | 'h264' | 'hevc' | 'av1'; preset: string; bitrate_mbps: number;
  replay_seconds: number; buffer_mb: number; output_dir: string; audio: AudioSource[]; separate_tracks: boolean;
  replay_hotkey: string; recording_hotkey: string; mute_hotkey: string; pause_hotkey: string;
  close_to_tray: boolean; diagnostics: boolean; check_updates: boolean; gpu_scaling: boolean;
  audio_bitrate_kbps: number; launch_at_startup: boolean; organize_by_game: boolean; game_mappings: GameMapping[];
  game_notifications: boolean; settings_version: number; scale_policy: 'smooth' | 'exact';
  overlay: OverlaySettings; tray_hint_shown: boolean;
};
export type OverlayPosition = 'bottom-right' | 'bottom-left' | 'top-right' | 'top-left';
export type OverlaySettings = {
  enabled: boolean; position: OverlayPosition; opacity: number; compact: boolean;
  show_replay: boolean; show_mic: boolean; show_timer: boolean; games_only: boolean;
};
export type Status = {
  state: string; replay: boolean; recording: boolean; saving: number; encoder: string; capture: string;
  buffer_seconds: number; buffer_bytes: number; free_bytes: number; frames: number; dropped_frames: number;
  fps: number; elapsed: number; write_mbps: number; memory_mb: number; cpu_percent: number;
  audio_peaks: number[]; audio_packets: number[]; audio_gaps: number[]; audio_discontinuities: number[];
  audio_drift_ppm: number[]; audio_written_bytes: number[]; error: string | null; notice: string | null;
  last_clip: string | null; recovery_sessions: number; geometry: Display | null; scaling: string;
  scaling_detail: string | null; output_width: number; output_height: number; segment_age_ms: number;
  frame_age_ms: number; hotkey_error: string | null;
  duplicated_frames: number; detected_game: string | null; last_save_timings: { stage: string; elapsed_ms: number }[];
  pending_recording: boolean; recording_elapsed: number; start_timings: { stage: string; elapsed_ms: number }[];
  encoder_speed: number; video_lag_ms: number; target_bitrate_mbps: number; encoder_label: string; foreground_game: string | null; play_width: number; play_height: number;
  cap_bitrate_mbps: number; constant_quality: number | null; codec: string;
};
export type Probe = {
  duration: number; bytes: number; video_codec: string; audio_tracks: number; width: number; height: number;
  start_time: number; audio_titles: string[]; fps: number; game?: string | null; kind?: string | null;
};
/** `id` is the path relative to the clips folder: `Fortnite/Fortnite_….mkv` or a bare legacy name. */
export type Clip = { id: string; name: string; category: string; kind: string; modified: number; bytes: number; probe: Probe | null };
export type GameMapping = { exe: string; name: string };
export type Encoder = { name: string; available: boolean; reason: string };
export type Device = { id: string; name: string; kind: 'system' | 'microphone' };
export type Caps = {
  encoders: Encoder[]; codecs?: { codec: 'h264' | 'hevc' | 'av1'; encoders: string[] }[]; media_error: string | null; displays: Display[]; devices: Device[]; updates_configured: boolean;
};
export type Session = { pid: number; name: string; path: string; started: number; active: boolean };
export type TestCapture = {
  seconds: number; packets: number; peak: number; rms: number; discontinuities: number; device_glitches: number;
  timestamp_errors: number; drift_correction_ppm: number; measured_device_clock_ppm: number;
  endpoint_mix_format: string | null;
};
export type UpdateInfo = { available: boolean; version?: string; notes?: string };
export type ActionName = 'start_replay' | 'stop_replay' | 'start_recording' | 'stop_recording' | 'save_replay' | 'stop_all' | 'recover' | 'discard_sessions';

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!native) return Promise.reject(new Error('Open the REPLAYX desktop app to use this control.'));
  return invoke<T>(cmd, args);
}

export const api = {
  status: () => call<Status>('status'),
  settings: () => call<Settings>('settings'),
  saveSettings: (settings: Settings) => call<void>('save_settings', { settings }),
  action: <T = unknown>(name: ActionName) => call<T>('action', { name }),
  setMix: (index: number, volume: number, muted: boolean) => call<void>('set_mix', { index, volume, muted }),
  capabilities: () => call<Caps>('capabilities'),
  sessions: () => call<Session[]>('audio_sessions'),
  icon: (path: string) => call<string | null>('app_icon', { path }),
  clips: () => call<Clip[]>('clips'),
  thumbnail: (id: string) => call<string>('clip_thumbnail', { id }),
  /** Returns the clip's id after the operation (new id for rename/move/mp4). */
  clipAction: (id: string, operation: 'delete' | 'rename' | 'move' | 'folder' | 'play' | 'mp4', value?: string) =>
    call<string>('clip_action', { id, operation, value: value ?? null }),
  audioTest: (source: AudioSource, seconds: number) => call<TestCapture>('audio_test', { source, seconds }),
  audioTestStop: () => call<void>('audio_test_stop'),
  audioTestData: () => call<string>('audio_test_data'),
  audioTestDiscard: () => call<void>('audio_test_discard'),
  exitApp: () => call<void>('exit_app'),
  audioDevices: () => call<Device[]>('audio_devices'),
  micMonitorStart: (source: AudioSource, output: string) => call<void>('mic_monitor_start', { source, output }),
  micMonitorStop: () => call<void>('mic_monitor_stop'),
  openFolder: () => call<void>('open_folder'),
  checkUpdate: () => call<UpdateInfo>('check_update'),
  installUpdate: () => call<void>('install_update'),
};

export const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export const emptyStatus: Status = {
  state: 'Stopped', replay: false, recording: false, saving: 0, encoder: '', capture: '', buffer_seconds: 0,
  buffer_bytes: 0, free_bytes: 0, frames: 0, dropped_frames: 0, fps: 0, elapsed: 0, write_mbps: 0, memory_mb: 0,
  cpu_percent: 0, audio_peaks: [], audio_packets: [], audio_gaps: [], audio_discontinuities: [], audio_drift_ppm: [],
  audio_written_bytes: [], error: null, notice: null, last_clip: null, recovery_sessions: 0, geometry: null,
  scaling: '', scaling_detail: null, output_width: 0, output_height: 0, segment_age_ms: 0, frame_age_ms: 0,
  hotkey_error: null, duplicated_frames: 0, detected_game: null, last_save_timings: [],
  pending_recording: false, recording_elapsed: 0, start_timings: [],
  encoder_speed: 0, video_lag_ms: 0, target_bitrate_mbps: 0, encoder_label: '', foreground_game: null, play_width: 0, play_height: 0,
  cap_bitrate_mbps: 0, constant_quality: null, codec: '',
};

/** Browser-preview defaults; the native app always loads real settings. */
export const previewSettings: Settings = {
  capture: 'desktop-duplication', display: 0, window_title: '', width: 1920, height: 1080, canvas_mode: 'stretch', source_rect: null,
  fps: 60, encoder: 'auto', codec: 'auto', preset: 'balanced', bitrate_mbps: 12, replay_seconds: 60, buffer_mb: 2048,
  output_dir: 'C:\\Users\\You\\Videos\\REPLAYX', audio: [], separate_tracks: true, replay_hotkey: 'Control+Shift+F10',
  recording_hotkey: 'Control+Shift+F9', mute_hotkey: 'Control+Shift+M', pause_hotkey: 'Control+Shift+F8',
  close_to_tray: false, diagnostics: false, check_updates: false, gpu_scaling: true, audio_bitrate_kbps: 192,
  launch_at_startup: false, organize_by_game: true, game_mappings: [], game_notifications: true, settings_version: 2, scale_policy: 'smooth',
  overlay: { enabled: true, position: 'bottom-right', opacity: 85, compact: false, show_replay: true, show_mic: true, show_timer: true, games_only: true },
  tray_hint_shown: false,
};

export const VERSION = '0.2.2-alpha.1';
