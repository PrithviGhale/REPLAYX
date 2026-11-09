import { useEffect, useState, useSyncExternalStore } from 'react';
import { FolderOpen, Layers, Monitor } from 'lucide-react';
import { api, native, type GameArt } from './api';

/** Game artwork for the session: the user's image, else the icon of the
 * game's executable seen on this PC, else a drawn default. Loaded once per
 * game, only for games the user actually has. Nothing is downloaded. */
const art = new Map<string, GameArt | null>();
const listeners = new Set<() => void>();
let version = 0;
const changed = () => { version++; listeners.forEach(l => l()); };
const SPECIAL = new Set(['Desktop', 'Mixed', 'Unsorted']);

let queue = new Set<string>();
let timer: number | undefined;
function request(names: string[]) {
  for (const n of names) if (!art.has(n) && !SPECIAL.has(n)) queue.add(n);
  if (!queue.size || !native || timer !== undefined) return;
  timer = window.setTimeout(() => {
    const batch = [...queue];
    queue = new Set();
    timer = undefined;
    batch.forEach(n => art.set(n, null));
    api.gameArt(batch).then(map => { Object.entries(map).forEach(([k, v]) => art.set(k, v)); changed(); }).catch(() => undefined);
  }, 30);
}
/** Forget one game's art (after choosing or removing an image). */
export function refreshArt(name: string) { art.delete(name); request([name]); changed(); }

export function useGameArt(names: string[]) {
  const key = names.join('\u0000');
  useEffect(() => { request(names); }, [key]); // eslint-disable-line react-hooks/exhaustive-deps
  useSyncExternalStore(l => { listeners.add(l); return () => { listeners.delete(l); }; }, () => version);
  return (name: string) => art.get(name) ?? null;
}

const hue = (name: string) => { let h = 0; for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360; return h; };
const initials = (name: string) => name.replace(/[^A-Za-z0-9 ]/g, ' ').split(/\s+/).filter(Boolean).slice(0, 2).map(w => w[0]).join('').toUpperCase() || '?';

/** A game's icon at `size` px: user image, executable icon or default tile. */
export function GameIcon({ name, size = 20, className = '' }: { name: string; size?: number; className?: string }) {
  const get = useGameArt([name]);
  const a = get(name);
  const [broken, setBroken] = useState(false);
  const src = !broken ? (a?.custom ?? a?.icon ?? null) : null;
  const style = { width: size, height: size, ['--h' as string]: hue(name) };
  if (SPECIAL.has(name)) {
    const Icon = name === 'Desktop' ? Monitor : name === 'Mixed' ? Layers : FolderOpen;
    return <span className={`game-icon special ${className}`} style={style} aria-hidden="true"><Icon size={Math.round(size * 0.55)} /></span>;
  }
  return (
    <span className={`game-icon ${a?.custom ? 'custom' : ''} ${src ? '' : 'default'} ${className}`} style={style} aria-hidden="true">
      {src ? <img src={src} alt="" draggable={false} onError={() => setBroken(true)} /> : <b style={{ fontSize: Math.max(8, Math.round(size * 0.36)) }}>{initials(name)}</b>}
    </span>
  );
}
