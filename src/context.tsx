import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { api, errorText, native, previewSettings, type Caps, type Clip, type Settings } from './api';
import { firstPoll, refreshNow } from './store';
import { splashDone, splashStep } from './splash';
import { listen } from '@tauri-apps/api/event';
import { applySaveEvent, type SaveEvent } from './save-events';
import { removeClip, upsertClip } from './library';
import { describeError, type FriendlyError } from './errors';

/** Errors carry their friendly form; the same problem again bumps `count`
 * instead of stacking another toast. */
type Toast = { id: number; kind: 'success' | 'error' | 'info'; text: string; error?: FriendlyError; count: number };
type Ctx = {
  saved: Settings; draft: Settings; dirty: boolean; loaded: boolean;
  patch: (p: Partial<Settings>) => void; save: () => Promise<boolean>; discard: () => void;
  /** Volume/mute apply immediately (even while capturing) and persist in the engine. */
  setMix: (index: number, volume: number, muted: boolean) => Promise<void>;
  /** The one microphone switch (home page, tray, shortcut). Resolves to the
   * state the engine confirmed; the interface never shows On before that. */
  setMicrophone: (on: boolean) => Promise<boolean>;
  /** Take over fields the engine changed and confirmed (never marks them unsaved). */
  syncFromEngine: (fields: Partial<Settings>) => void;
  caps: Caps | null; refreshCaps: () => Promise<void>;
  clips: Clip[]; refreshClips: () => Promise<void>;
  /** Apply a finished delete/move locally; no rescan needed. */
  dropClip: (id: string) => void; putClip: (clip: Clip) => void;
  pendingSaves: SaveEvent[];
  busy: string | null; run: <T>(label: string, work: () => Promise<T>, success?: string) => Promise<T | undefined>;
  toast: (kind: Toast['kind'], text: string) => void; toasts: Toast[]; dismiss: (id: number) => void;
};
const AppContext = createContext<Ctx | null>(null);

export function AppProvider({ children }: { children: React.ReactNode }) {
  const [saved, setSaved] = useState<Settings>(previewSettings);
  const [draft, setDraft] = useState<Settings>(previewSettings);
  const [loaded, setLoaded] = useState(!native);
  const [caps, setCaps] = useState<Caps | null>(null);
  const [clips, setClips] = useState<Clip[]>([]);
  const [pendingSaves, setPendingSaves] = useState<SaveEvent[]>([]);
  const libraryRevision = useRef(0);
  const [busy, setBusy] = useState<string | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);

  const dismiss = useCallback((id: number) => setToasts(t => t.filter(x => x.id !== id)), []);
  const toast = useCallback((kind: Toast['kind'], text: string) => {
    const error = kind === 'error' ? describeError(text) : undefined;
    const id = nextId.current++;
    setToasts(t => {
      const same = t.find(x => (error ? x.error?.key === error.key : x.kind === kind && x.text === text));
      if (same) return t.map(x => (x === same ? { ...x, count: x.count + 1 } : x));
      return [...t.slice(-3), { id, kind, text, error, count: 1 }];
    });
    if (kind !== 'error') window.setTimeout(() => dismiss(id), 5000);
  }, [dismiss]);

  const refreshCaps = useCallback(async () => { if (native) setCaps(await api.capabilities()); }, []);
  const refreshClips = useCallback(async () => {
    if (!native) return;
    const revision = libraryRevision.current;
    const result = await api.clips();
    if (revision === libraryRevision.current) setClips(result);
  }, []);

  useEffect(() => {
    if (!native) return;
    let closed = false;
    const subscription = listen<SaveEvent>('clip-save', ({ payload: event }) => {
      if (closed) return;
      setPendingSaves(p => applySaveEvent(p, event));
      if (event.stage === 'Failed') toast('error', `Save failed: ${event.error ?? 'unknown error'}. The footage was kept for recovery.`);
      if (event.stage === 'Ready' && event.clip) {
        // Ready means validated and playable; it is never sent earlier.
        const c = event.clip;
        toast('success', `${c.category === 'Unsorted' ? c.kind : `${c.category} ${c.kind.toLowerCase()}`} saved · ready in ${(event.elapsed_ms / 1000).toFixed(1)} s`);
        libraryRevision.current++;
        setClips(c => upsertClip(c, event.clip!));
        refreshNow();
      }
    });
    // Microphones and outputs plugged in or removed (checked every 2 s).
    const devices = listen<Caps['devices']>('audio-devices', ({ payload }) => {
      if (closed) return;
      setCaps(c => (c ? { ...c, devices: payload } : c));
    });
    // Background metadata for clips the index had not seen yet.
    const indexed = listen<Clip[]>('library-updated', ({ payload }) => {
      if (closed) return;
      libraryRevision.current++;
      setClips(payload);
    });
    // Microphone switched from the tray or with the shortcut.
    const mix = listen<Settings['audio']>('audio-settings', ({ payload }) => {
      if (closed) return;
      const apply = (s: Settings) => ({ ...s, audio: s.audio.map((a, i) => (payload[i] && payload[i].kind === a.kind ? { ...a, muted: payload[i].muted, enabled: payload[i].enabled } : a)) });
      setSaved(apply); setDraft(apply);
    });
    return () => {
      closed = true;
      void mix.then(unlisten => unlisten());
      void subscription.then(unlisten => unlisten());
      void indexed.then(unlisten => unlisten());
      void devices.then(unlisten => unlisten());
    };
  }, [toast]);
  const dropClip = useCallback((id: string) => { libraryRevision.current++; setClips(c => removeClip(c, id)); }, []);
  const putClip = useCallback((clip: Clip) => { libraryRevision.current++; setClips(c => upsertClip(c, clip)); }, []);

  useEffect(() => {
    if (!native) return;
    // Startup: each splash step is shown only while it is really happening.
    splashStep('Loading settings');
    api.settings()
      .then(s => { setSaved(s); setDraft(s); setLoaded(true); splashStep('Checking recording engine'); return Promise.race([firstPoll, new Promise(r => window.setTimeout(r, 8000))]); })
      .catch(e => toast('error', errorText(e)))
      .finally(() => splashDone());
    refreshCaps().catch(e => toast('error', errorText(e)));
    refreshClips().catch(e => toast('error', errorText(e)));
  }, [refreshCaps, refreshClips, toast]);

  const run = useCallback(async <T,>(label: string, work: () => Promise<T>, success?: string) => {
    setBusy(label);
    try {
      const result = await work();
      if (success) toast('success', success);
      return result;
    } catch (e) {
      toast('error', errorText(e));
      return undefined;
    } finally {
      setBusy(null);
      refreshNow();
    }
  }, [toast]);

  const dirty = useMemo(() => JSON.stringify(saved) !== JSON.stringify(draft), [saved, draft]);
  const patch = useCallback((p: Partial<Settings>) => setDraft(d => ({ ...d, ...p })), []);
  const discard = useCallback(() => setDraft(saved), [saved]);
  const save = useCallback(async () => {
    const ok = await run('Saving settings', async () => {
      const stored = await api.saveSettings(draft);
      // A shortcut another program owns was not taken: say which one stayed.
      const keys = ['replay_hotkey', 'recording_hotkey', 'pause_hotkey', 'mute_hotkey', 'window_hotkey'] as const;
      const refused = keys.filter(k => stored[k] !== draft[k]);
      if (refused.length) toast('error', `${refused.map(k => draft[k].replace('Control', 'Ctrl')).join(', ')} is already used by another application. Change that application's hotkey or choose another shortcut. Your previous shortcut still works.`);
      return true;
    }, 'Settings saved.');
    if (ok) {
      // Re-read so the UI reflects exactly what the engine stored.
      const fresh = await api.settings().catch(() => draft);
      setSaved(fresh); setDraft(fresh);
      await refreshClips().catch(() => undefined);
    }
    return !!ok;
  }, [draft, run, refreshClips]);

  const setMix = useCallback(async (index: number, volume: number, muted: boolean) => {
    const apply = (s: Settings) => ({ ...s, audio: s.audio.map((a, i) => (i === index ? { ...a, volume, muted } : a)) });
    setSaved(apply); setDraft(apply);
    if (native) await api.setMix(index, volume, muted).catch(e => toast('error', errorText(e)));
  }, [toast]);

  const syncFromEngine = useCallback((fields: Partial<Settings>) => {
    setSaved(s => ({ ...s, ...fields })); setDraft(s => ({ ...s, ...fields }));
  }, []);
  const setMicrophone = useCallback(async (on: boolean) => {
    const before = saved.audio.find(a => a.kind === 'microphone');
    const was = !!before && before.enabled && !before.muted;
    if (!native) return was;
    try {
      const confirmed = await api.setMicrophone(on);
      const apply = (s: Settings) => ({ ...s, audio: s.audio.map((a, i) => (confirmed.audio[i] && confirmed.audio[i].kind === a.kind ? { ...a, muted: confirmed.audio[i].muted, enabled: confirmed.audio[i].enabled } : a)) });
      setSaved(apply); setDraft(apply);
      const mic = confirmed.audio.find(a => a.kind === 'microphone');
      return !!mic && mic.enabled && !mic.muted;
    } catch (e) {
      toast('error', errorText(e));
      return was;
    }
  }, [saved.audio, toast]);

  const value = { setMix, setMicrophone, syncFromEngine, saved, draft, dirty, loaded, patch, save, discard, caps, refreshCaps, clips, refreshClips, dropClip, putClip, pendingSaves, busy, run, toast, toasts, dismiss };
  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useApp() {
  const c = useContext(AppContext);
  if (!c) throw new Error('AppProvider missing');
  return c;
}
