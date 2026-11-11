import type { AudioSource, UpdateSettings, UpdateStatus } from './api';

/** Pure helpers for the update interface (no React, no engine calls). */

export type UpdateMode = 'manual' | 'notify' | 'auto' | 'custom';
export const MODES: { id: Exclude<UpdateMode, 'custom'>; label: string; text: string; recommended?: boolean }[] = [
  { id: 'manual', label: 'Manual', text: 'Only check when I click Check now.' },
  { id: 'notify', label: 'Notify me', text: 'Check automatically and ask before installing.', recommended: true },
  { id: 'auto', label: 'Auto update', text: 'Download and install by itself when it is safe.' },
];
export const RECOMMENDED_HOURS = 6;
export const INTERVALS = [1, 3, 6, 12, 24];
export const IDLE_MINUTES = [2, 5, 15, 30];

/** Same rules as the engine: installing needs downloading, which needs checking. */
export function normalize(s: UpdateSettings): UpdateSettings {
  const auto_download = s.auto_check && s.auto_download;
  return { ...s, auto_download, auto_install: auto_download && s.auto_install };
}
export function modeOf(s: UpdateSettings): UpdateMode {
  if (!s.auto_check && !s.auto_download && !s.auto_install) return 'manual';
  if (s.auto_check && s.notify && !s.auto_download && !s.auto_install) return 'notify';
  if (s.auto_check && s.auto_download && s.auto_install) return 'auto';
  return 'custom';
}
export function applyMode(s: UpdateSettings, mode: Exclude<UpdateMode, 'custom'>): UpdateSettings {
  if (mode === 'manual') return { ...s, auto_check: false, auto_download: false, auto_install: false };
  if (mode === 'notify') return { ...s, auto_check: true, notify: true, auto_download: false, auto_install: false };
  return { ...s, auto_check: true, auto_download: true, auto_install: true };
}

/** A newer version is known: drives the Settings dot and the Updates badge. */
export function hasUpdate(s: UpdateStatus): boolean {
  return !!s.latest && ['available', 'downloading', 'verifying', 'ready', 'installing'].includes(s.stage);
}
/** Real download progress, or null while the size is unknown. */
export function percent(s: Pick<UpdateStatus, 'downloaded' | 'total'>): number | null {
  if (!s.total || s.total <= 0) return null;
  return Math.max(0, Math.min(100, Math.floor((s.downloaded / s.total) * 100)));
}
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}
export function progressText(s: Pick<UpdateStatus, 'downloaded' | 'total'>): string {
  const p = percent(s);
  return p === null ? `${formatBytes(s.downloaded)} downloaded` : `${p}% · ${formatBytes(s.downloaded)} of ${formatBytes(s.total!)}`;
}
export function stageLabel(s: UpdateStatus): string {
  switch (s.stage) {
    case 'checking': return 'Checking for updates…';
    case 'up_to_date': return 'REPLAYX is up to date';
    case 'available': return `Update available — v${s.latest}`;
    case 'downloading': return 'Downloading update…';
    case 'verifying': return 'Verifying the download…';
    case 'ready': return s.waiting ? 'Update ready — waiting for a safe time to install' : 'Update ready to install';
    case 'installing': return 'Installing… REPLAYX will restart';
    case 'failed': return 'Update failed';
    default: return 'No check yet';
  }
}
/** "What's new" lines: the first bullet points of the notes, without Markdown. */
export function highlights(notes: string | null, max = 4): string[] {
  if (!notes) return [];
  return notes.split('\n').map(l => l.trim()).filter(l => /^[-*] /.test(l))
    .map(l => l.slice(2).replace(/\*\*/g, '').replace(/`/g, '').trim()).filter(Boolean).slice(0, max);
}
export function lastChecked(ms: number | null, now = Date.now()): string {
  if (!ms) return 'Never';
  const ago = now - ms;
  if (ago < 60_000) return 'Just now';
  if (ago < 3_600_000) { const m = Math.floor(ago / 60_000); return `${m} minute${m === 1 ? '' : 's'} ago`; }
  const d = new Date(ms), today = new Date(now);
  const time = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  if (d.toDateString() === today.toDateString()) return `Today, ${time}`;
  return `${d.toLocaleDateString([], { month: 'short', day: 'numeric', year: 'numeric' })}, ${time}`;
}

/** Is the microphone heard in recordings? null when no microphone is set up. */
export function microphoneOn(audio: Pick<AudioSource, 'kind' | 'enabled' | 'muted'>[]): boolean | null {
  const mic = audio.find(a => a.kind === 'microphone');
  return mic ? mic.enabled && !mic.muted : null;
}
