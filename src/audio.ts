import type { AudioSource, VoiceConfig } from './api';

/** Sources whose identity matches the saved settings can be mixed live. */
export const identity = (a: AudioSource) => `${a.kind}|${a.device}|${a.pid}|${a.process_started}|${a.enabled}`;

/** Fallback for the "Clear" preset when the engine is not reachable (UI
 * preview). The engine's values (api.voicePresets) are authoritative. */
export const CLEAR_VOICE: VoiceConfig = {
  preset: 'clear', high_pass: true, suppression: 'balanced', gate: false, gate_threshold_db: -50,
  compressor: true, comp_threshold_db: -24, comp_ratio: 2.5, warmth_db: 0, mud_db: -1.5, presence_db: 2.5, air_db: 1, deesser: false,
};

/** A new source with the default Voice Enhancer settings (used for microphones). */
export function micDefaults(device: string): AudioSource {
  return {
    kind: 'microphone', device, pid: 0, process_started: 0, name: 'Microphone', enabled: true, volume: 1, muted: false,
    boost_db: 0, voice: { ...CLEAR_VOICE },
  };
}

export type MicTone = 'neutral' | 'ok' | 'warn' | 'bad' | 'accent' | 'rec';
/** What the microphone is actually doing, for badges and the in-game overlay.
 * "Recording" only while capture runs with the microphone in it; "Ready" when
 * it will be recorded once capture starts; never claims capture otherwise. */
export function micState({ mic, capturing, deviceKnown }: { mic: AudioSource | null; capturing: boolean; deviceKnown: boolean }):
  { key: 'none' | 'off' | 'muted' | 'missing' | 'ready' | 'recording'; label: string; tone: MicTone } {
  if (!mic) return { key: 'none', label: 'Not added', tone: 'neutral' };
  if (!mic.enabled) return { key: 'off', label: 'Off', tone: 'neutral' };
  if (!deviceKnown) return { key: 'missing', label: 'Disconnected', tone: 'warn' };
  if (mic.muted) return { key: 'muted', label: 'Muted', tone: 'warn' };
  if (capturing) return { key: 'recording', label: 'Recording', tone: 'rec' };
  return { key: 'ready', label: 'Ready', tone: 'ok' };
}
