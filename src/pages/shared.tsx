import React, { useEffect, useRef, useState } from 'react';
import { AppWindow, Film, Mic, Speaker, VolumeX } from 'lucide-react';
import { api, native, type AudioSource, type Clip } from '../api';

const thumbs = new Map<string, string>();
/** Clip still, requested only once visible and cached for the session. */
export function Thumb({ clip, className = '' }: { clip: Clip; className?: string }) {
  const key = `${clip.id}:${clip.modified}:${clip.bytes}`;
  const [url, setUrl] = useState<string | undefined>(thumbs.get(key));
  const [failed, setFailed] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (url || failed || !native || !ref.current) return;
    const el = ref.current;
    const io = new IntersectionObserver(entries => {
      if (!entries.some(e => e.isIntersecting)) return;
      io.disconnect();
      api.thumbnail(clip.id).then(u => { thumbs.set(key, u); setUrl(u); }).catch(() => setFailed(true));
    }, { rootMargin: '200px' });
    io.observe(el);
    return () => io.disconnect();
  }, [clip.id, key, url, failed]);
  return (
    <div ref={ref} className={`thumb ${className}`}>
      {url ? <img src={url} alt="" draggable={false} /> : <Film size={26} className={failed ? '' : 'pulse'} />}
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
