import { invoke } from '@tauri-apps/api/core';
import type { SettingsProblem } from './errors';

/** True inside the Tauri window; false in a plain browser (UI preview only). */
export const native = !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;

export type AudioKind = 'system' | 'microphone' | 'application' | 'exclude';
export type AudioSource = {
  kind: AudioKind; device: string; pid: number; process_started: number;
  name: string; enabled: boolean; volume: number; muted: boolean;
  boost_db: number; voice: VoiceConfig;
};
export type VoicePreset = 'raw' | 'natural' | 'clear' | 'broadcast' | 'noise_free' | 'custom';
export type Suppression = 'off' | 'light' | 'balanced' | 'strong';
/** Mirror of voice::VoiceConfig (the engine owns the preset values: api.voicePresets). */
export type VoiceConfig = {
  preset: VoicePreset; high_pass: boolean; suppression: Suppression; gate: boolean; gate_threshold_db: number;
  compressor: boolean; comp_threshold_db: number; comp_ratio: number;
  warmth_db: number; mud_db: number; presence_db: number; air_db: number; deesser: boolean;
};
export type SavedVoice = { name: string; voice: VoiceConfig; boost_db: number };
export type VoiceSample = {
  original: string; enhanced: string; input_peak_db: number; enhanced_peak_db: number; input_clipped: boolean;
  noise_floor_db: number; enhanced_noise_floor_db: number; match_db: number; seconds: number;
};
export type Rect = { x: number; y: number; width: number; height: number };
export type Display = {
  adapter_vendor?: number;
  index: number; device: string; adapter_luid: string; active_width: number; active_height: number;
  desktop_width: number; desktop_height: number; refresh_hz: number; rotation: number; left: number; top: number;
};
export type CanvasMode = 'stretch' | 'fit' | 'crop' | 'native';
export type Settings = {
  capture: string; display: number; window_title: string; width: number; height: number; canvas_mode: CanvasMode;
  source_rect: Rect | null; fps: number; encoder: string; codec: 'auto' | 'h264' | 'hevc' | 'av1'; preset: string; bitrate_mbps: number;
  replay_seconds: number; buffer_mb: number; output_dir: string; audio: AudioSource[]; separate_tracks: boolean;
  replay_hotkey: string; recording_hotkey: string; mute_hotkey: string; pause_hotkey: string; window_hotkey: string;
  close_to_tray: boolean; diagnostics: boolean; check_updates: boolean; gpu_scaling: boolean; capture_api: string;
  audio_bitrate_kbps: number; launch_at_startup: boolean; organize_by_game: boolean; game_mappings: GameMapping[];
  game_notifications: boolean; settings_version: number; scale_policy: 'smooth' | 'exact';
  overlay: OverlaySettings; tray_hint_shown: boolean; tour_done: boolean; voice_presets: SavedVoice[];
  playtime: PlaytimeSettings; capture_notices: boolean; updates: UpdateSettings;
};
export type OverlayPosition = 'bottom-right' | 'bottom-left' | 'top-right' | 'top-left';
export type OverlaySettings = {
  enabled: boolean; position: OverlayPosition; opacity: number; show_replay: boolean; show_mic: boolean;
  show_recording: boolean; show_timer: boolean;
};
export type PlaytimeSettings = { enabled: boolean; paused: boolean; idle_minutes: number; excluded: string[] };
export type PlaySession = { id: string; game: string; start: number; seconds: number; source: string };
export type PlaytimeData = {
  sessions: PlaySession[]; current: { game: string; start: number; seconds: number } | null; settings: PlaytimeSettings;
};
export type GameArt = { custom: string | null; icon: string | null; id: string | null };
export type Status = {
  state: string; replay: boolean; recording: boolean; saving: number; encoder: string; capture: string;
  buffer_seconds: number; buffer_bytes: number; free_bytes: number; frames: number; dropped_frames: number;
  fps: number; elapsed: number; write_mbps: number; memory_mb: number; cpu_percent: number;
  audio_peaks: number[]; audio_packets: number[]; audio_gaps: number[]; audio_discontinuities: number[];
  audio_drift_ppm: number[]; audio_written_bytes: number[]; audio_queue_ms?: number[]; error: string | null;
  /** Engine actions that redo what `error` interrupted (empty: no Retry). */
  error_retry: string[]; notice: string | null;
  /** What start-up found wrong with settings.json. */
  settings_problem: SettingsProblem | null;
  last_clip: string | null; recovery_sessions: number; geometry: Display | null; scaling: string;
  capture_api: string; capture_filter: string; capture_fallback: string | null; scaling_detail: string | null; output_width: number; output_height: number; segment_age_ms: number;
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
export type UpdateSettings = {
  auto_check: boolean; interval_hours: number; auto_download: boolean; auto_install: boolean; notify: boolean; idle_minutes: number;
};
export type UpdateStage = 'idle' | 'checking' | 'up_to_date' | 'available' | 'downloading' | 'verifying' | 'ready' | 'installing' | 'failed';
/** Sent by the engine (`update-status` event). `simulated` marks test data. */
export type UpdateStatus = {
  /** Increases with every change; an older status is never applied over a newer one. */
  revision: number;
  stage: UpdateStage; current: string; latest: string | null; notes: string | null; date: string | null;
  downloaded: number; total: number | null; error: string | null; detail: string | null; last_checked: number | null;
  simulated: boolean; configured: boolean; installable: boolean; waiting: boolean; prompt: boolean; release_url: string | null;
};
export type ActionName = 'start_replay' | 'stop_replay' | 'start_recording' | 'stop_recording' | 'save_replay' | 'stop_all' | 'recover' | 'discard_sessions';

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!native) return Promise.reject(new Error('Open the REPLAYX desktop app to use this control.'));
  return invoke<T>(cmd, args);
}

export const api = {
  status: () => call<Status>('status'),
  settings: () => call<Settings>('settings'),
  /** Returns the settings as stored, with the shortcuts actually active. */
  saveSettings: (settings: Settings) => call<Settings>('save_settings', { settings }),
  testHotkey: (combo: string) => call<void>('test_hotkey', { combo }),
  retryHotkeys: () => call<string | null>('retry_hotkeys'),
  gameArt: (games: string[]) => call<Record<string, GameArt>>('game_art', { games }),
  gameArtSet: (game: string, image: string) => call<void>('game_art_set', { game, image }),
  gameArtClear: (game: string) => call<void>('game_art_clear', { game }),
  playtimeData: () => call<PlaytimeData>('playtime_data'),
  playtimeConfigure: (settings: PlaytimeSettings) => call<void>('playtime_configure', { settings }),
  playtimeExport: () => call<string>('playtime_export'),
  playtimeClear: () => call<void>('playtime_clear'),
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
  audioTestStop: () => call<void>('audio_test_stop'),
  voicePresets: () => call<Record<string, VoiceConfig>>('voice_presets'),
  voiceSampleRecord: (source: AudioSource, seconds: number) => call<number>('voice_sample_record', { source, seconds }),
  voiceSampleRender: (voice: VoiceConfig, boostDb: number) => call<VoiceSample>('voice_sample_render', { voice, boostDb }),
  voiceSampleDiscard: () => call<void>('voice_sample_discard'),
  openSoundSettings: () => call<void>('open_sound_settings'),
  /** Fixed Windows Settings pages only (see `windows_settings_uri`). */
  openWindowsSettings: (page: 'sound' | 'microphone-privacy' | 'display' | 'storage') => call<void>('open_windows_settings', { page }),
  markTourDone: () => call<void>('mark_tour_done'),
  exitApp: () => call<void>('exit_app'),
  audioDevices: () => call<Device[]>('audio_devices'),
  micMonitorStart: (source: AudioSource, output: string) => call<void>('mic_monitor_start', { source, output }),
  micMonitorStop: () => call<void>('mic_monitor_stop'),
  micMonitorSet: (volume: number, voice: VoiceConfig, boostDb: number, original: boolean) =>
    call<void>('mic_monitor_set', { volume, voice, boostDb, original }),
  openFolder: () => call<void>('open_folder'),
  updateStatus: () => call<UpdateStatus>('update_status'),
  checkUpdate: () => call<UpdateStatus>('check_update'),
  updateDownload: (install: boolean) => call<void>('update_download', { install }),
  installUpdate: (pauseReplay: boolean) => call<void>('install_update', { pauseReplay }),
  updateDismiss: () => call<UpdateStatus>('update_dismiss'),
  updateConfigure: (settings: UpdateSettings) => call<Settings>('update_configure', { settings }),
  updateOpenNotes: () => call<void>('update_open_notes'),
  updateSimulate: (scenario: string) => call<UpdateStatus>('update_simulate', { scenario }),
  setMicrophone: (on: boolean) => call<Settings>('set_microphone', { on }),
};

export const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export const emptyStatus: Status = {
  state: 'Stopped', replay: false, recording: false, saving: 0, encoder: '', capture: '', buffer_seconds: 0,
  buffer_bytes: 0, free_bytes: 0, frames: 0, dropped_frames: 0, fps: 0, elapsed: 0, write_mbps: 0, memory_mb: 0,
  cpu_percent: 0, audio_peaks: [], audio_packets: [], audio_gaps: [], audio_discontinuities: [], audio_drift_ppm: [],
  audio_written_bytes: [], error: null, error_retry: [], notice: null, settings_problem: null, last_clip: null, recovery_sessions: 0, geometry: null,
  scaling: '', capture_api: '', capture_filter: '', capture_fallback: null, scaling_detail: null, output_width: 0, output_height: 0, segment_age_ms: 0, frame_age_ms: 0,
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
  recording_hotkey: 'Control+Shift+F9', mute_hotkey: 'Control+Shift+M', pause_hotkey: 'Control+Shift+F8', window_hotkey: 'Control+Shift+F11',
  close_to_tray: true, diagnostics: false, check_updates: false, gpu_scaling: true, capture_api: 'dda', audio_bitrate_kbps: 192,
  launch_at_startup: false, organize_by_game: true, game_mappings: [], game_notifications: true, settings_version: 6, scale_policy: 'smooth',
  overlay: { enabled: true, position: 'bottom-right', opacity: 90, show_replay: true, show_mic: true, show_recording: true, show_timer: false },
  playtime: { enabled: true, paused: false, idle_minutes: 15, excluded: [] }, capture_notices: true,
  updates: { auto_check: true, interval_hours: 6, auto_download: false, auto_install: false, notify: true, idle_minutes: 5 },
  tray_hint_shown: false, tour_done: false, voice_presets: [],
};

export const VERSION = '0.3.1-alpha.9';
