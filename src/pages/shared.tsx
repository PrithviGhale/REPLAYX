import React, { useEffect, useRef, useState } from 'react';
import { AppWindow, Film, Mic, Speaker, VolumeX } from 'lucide-react';
import { api, native, type AudioSource, type Clip } from '../api';
import { applies, cachedThumb, loadThumb, thumbKey } from '../thumbs';

/** Clip still, requested only once visible. The image always belongs to the
 * clip shown: switching clips shows a placeholder until the new clip's own
 * image arrives, and a late answer for a previous clip is ignored. */
export function Thumb({ clip, className = '' }: { clip: Clip; className?: string }) {
  const key = thumbKey(clip);
  const [shown, setShown] = useState<{ key: string; url?: string; failed?: boolean }>(() => ({ key, url: cachedThumb(key) }));
  const current = useRef(key);
  current.current = key;
  const ref = useRef<HTMLDivElement>(null);
  // A different clip: never keep the previous clip's image on screen.
  const state = shown.key === key ? shown : { key, url: cachedThumb(key) };
  useEffect(() => {
    if (shown.key !== key) setShown({ key, url: cachedThumb(key) });
  }, [key, shown.key]);
  useEffect(() => {
    if (state.url || state.failed || !native || !ref.current) return;
    const el = ref.current;
    const io = new IntersectionObserver(entries => {
      if (!entries.some(e => e.isIntersecting)) return;
      io.disconnect();
      loadThumb(key, () => api.thumbnail(clip.id))
        .then(url => { if (applies(key, current.current)) setShown({ key, url }); })
        .catch(() => { if (applies(key, current.current)) setShown({ key, failed: true }); });
    }, { rootMargin: '200px' });
    io.observe(el);
    return () => io.disconnect();
  }, [clip.id, key, state.url, state.failed]);
  return (
    <div ref={ref} className={`thumb ${className}`} data-clip={clip.id}>
      {state.url ? <img key={key} src={state.url} alt="" draggable={false} /> : <Film size={26} className={state.failed ? '' : 'pulse'} />}
    </div>
  );
}

const icons = new Map<string, string | null>();
/** Application icon from the executable, or a kind icon. */
export function SourceIcon({ source, path }: { source: Pick<AudioSource, 'kind' | 'muted'>; path?: string }) {
  const [url, setUrl] = useState<string | null | undefined>(path ? icons.get(path) : null);
  useEffect(() => {
    if (!path || icons.has(path) || !native) return;
    api.icon(path).then(u => { icons.set(path, u); setUrl(u); }).catch(() => icons.set(path, null));
  }, [path]);
  const Fallback = source.kind === 'microphone' ? Mic : source.kind === 'system' ? Speaker : AppWindow;
  return (
    <span className={`source-icon kind-${source.kind}`}>
      {url ? <img src={url} alt="" /> : <Fallback size={18} />}
      {source.muted && <span className="muted-mark"><VolumeX size={10} /></span>}
    </span>
  );
}

export const kindLabel: Record<string, string> = {
  system: 'System output', microphone: 'Microphone', application: 'Application', exclude: 'System except app',
};

/** Index of a source among enabled sources = its meter/feed index. */
export const feedIndex = (sources: AudioSource[], i: number) => sources.slice(0, i).filter(s => s.enabled).length;

export function Section({ children }: { children: React.ReactNode }) {
  return <div className="page">{children}</div>;
}
