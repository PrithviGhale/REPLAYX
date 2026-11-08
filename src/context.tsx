import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { api, errorText, native, previewSettings, type Caps, type Clip, type Settings } from './api';
import { firstPoll, refreshNow } from './store';
import { splashDone, splashStep } from './splash';
import { listen } from '@tauri-apps/api/event';
import { applySaveEvent, type SaveEvent } from './save-events';
import { removeClip, upsertClip } from './library';

type Toast = { id: number; kind: 'success' | 'error' | 'info'; text: string };
type Ctx = {
  saved: Settings; draft: Settings; dirty: boolean; loaded: boolean;
  patch: (p: Partial<Settings>) => void; save: () => Promise<boolean>; discard: () => void;
  /** Volume/mute apply immediately (even while capturing) and persist in the engine. */
  setMix: (index: number, volume: number, muted: boolean) => Promise<void>;
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
    const id = nextId.current++;
    setToasts(t => [...t.slice(-3), { id, kind, text }]);
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
    return () => {
      closed = true;
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
    const ok = await run('Saving settings', async () => { await api.saveSettings(draft); return true; }, 'Settings saved.');
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

  const value = { setMix, saved, draft, dirty, loaded, patch, save, discard, caps, refreshCaps, clips, refreshClips, dropClip, putClip, pendingSaves, busy, run, toast, toasts, dismiss };
  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useApp() {
  const c = useContext(AppContext);
  if (!c) throw new Error('AppProvider missing');
  return c;
}
