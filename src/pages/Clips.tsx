import { useEffect, useMemo, useRef, useState } from 'react';
import { CalendarRange, FileVideo, FolderOpen, Gamepad2, LayoutGrid, List, LoaderCircle, Pencil, Play, RefreshCw, Search, Share2, Trash2, X } from 'lucide-react';
import { api, errorText, type Clip } from '../api';
import { useApp } from '../context';
import { bytes, duration } from '../format';
import { ALL, SORTS, categories, filterClips, gameTiles, nextSelection, type Sort } from '../library';
import { Badge, Button, Card, Empty, IconButton, Modal, Segmented } from '../ui';
import { Section, Thumb } from './shared';
import { GameIcon } from '../gameArt';
import { forgetThumbs } from '../thumbs';

const stem = (n: string) => n.replace(/\.(mkv|mp4)$/i, '');
const ext = (n: string) => n.match(/\.(mkv|mp4)$/i)?.[0] ?? '';
const PAGE = 60;
const gameLabel = (c: Clip) => (c.category === 'Unsorted' ? (c.probe?.game ?? 'Unsorted') : c.category);
const kindLabel = (k: string) => (k === 'Recording' ? 'Recording' : k === 'Replay' ? 'Replay' : k === 'Export' ? 'MP4' : k);

/** Library preferences, kept across page changes and launches. */
type Prefs = { view: 'all' | 'games'; sort: Sort; kind: string; game: string; from: string; to: string };
const DEFAULT_PREFS: Prefs = { view: 'all', sort: 'new', kind: 'all', game: ALL, from: '', to: '' };
const PREFS_KEY = 'replayx.clips.prefs';
function loadPrefs(): Prefs {
  try { return { ...DEFAULT_PREFS, ...JSON.parse(localStorage.getItem(PREFS_KEY) ?? '{}') }; } catch { return DEFAULT_PREFS; }
}
/** Open the library on one game (used by the Games page). */
export function showGameClips(game: string) {
  try { localStorage.setItem(PREFS_KEY, JSON.stringify({ ...loadPrefs(), view: 'all', game })); } catch { /* preview */ }
}

export function ClipsPage() {
  const { clips, refreshClips, dropClip, pendingSaves, run, busy, draft, toast } = useApp();
  const [prefs, setPrefsState] = useState<Prefs>(loadPrefs);
  const setPrefs = (p: Partial<Prefs>) => setPrefsState(old => {
    const next = { ...old, ...p };
    try { localStorage.setItem(PREFS_KEY, JSON.stringify(next)); } catch { /* storage unavailable */ }
    return next;
  });
  const [query, setQuery] = useState('');
  const [shown, setShown] = useState(PAGE);
  const [selected, setSelected] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<Clip | null>(null);
  const [newName, setNewName] = useState('');
  const [moving, setMoving] = useState<Clip | null>(null);
  const [target, setTarget] = useState('');
  const [deleting, setDeleting] = useState<Clip | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [deletingNow, setDeletingNow] = useState(false);
  const sentinel = useRef<HTMLDivElement>(null);

  const cats = useMemo(() => categories(clips), [clips]);
  const tiles = useMemo(() => gameTiles(clips), [clips]);
  const list = useMemo(() => filterClips(clips, prefs.game, prefs.kind, query, prefs.sort, { from: prefs.from, to: prefs.to }),
    [clips, prefs.game, prefs.kind, prefs.sort, prefs.from, prefs.to, query]);
  const total = useMemo(() => clips.reduce((n, c) => n + c.bytes, 0), [clips]);
  const clip = clips.find(c => c.id === selected) ?? null;
  useEffect(() => setShown(PAGE), [prefs.game, prefs.kind, prefs.sort, prefs.from, prefs.to, query]);
  // A game whose last clip was deleted or moved disappears from the filter.
  useEffect(() => { if (prefs.game !== ALL && !cats.some(c => c.name === prefs.game)) setPrefs({ game: ALL }); }, [cats, prefs.game]); // eslint-disable-line react-hooks/exhaustive-deps
  // Render large libraries incrementally as the user scrolls.
  useEffect(() => {
    const el = sentinel.current;
    if (!el) return;
    const io = new IntersectionObserver(e => { if (e.some(x => x.isIntersecting)) setShown(n => n + PAGE); }, { rootMargin: '600px' });
    io.observe(el);
    return () => io.disconnect();
  }, [list.length, shown, prefs.view]);

  const act = (c: Clip, op: 'play' | 'folder' | 'mp4', label: string, done?: string) =>
    run(label, async () => { const r = await api.clipAction(c.id, op); if (op === 'mp4') await refreshClips(); return r; }, done);

  const confirmDelete = async () => {
    if (!deleting) return;
    setDeletingNow(true); setDeleteError(null);
    try {
      await api.clipAction(deleting.id, 'delete');
      const next = nextSelection(list, deleting.id);
      dropClip(deleting.id);
      forgetThumbs(deleting.id);
      if (selected === deleting.id) setSelected(next);
      toast('success', `Moved ${deleting.name} to the Recycle Bin.`);
      setDeleting(null);
    } catch (e) {
      // Keep the dialog open with Windows' actual reason; nothing was removed.
      setDeleteError(errorText(e));
    } finally {
      setDeletingNow(false);
    }
  };
  const filtered = prefs.game !== ALL || prefs.kind !== 'all' || prefs.from || prefs.to || query;

  return (
    <Section>
      <div className="toolbar lib-toolbar">
        <Segmented label="View" value={prefs.view} onChange={v => setPrefs({ view: v })}
          options={[{ value: 'all' as const, label: <><List size={14} /> All clips</> }, { value: 'games' as const, label: <><LayoutGrid size={14} /> By game</> }]} />
        <div className="search"><Search size={15} /><input placeholder="Search game or file name" aria-label="Search clips" value={query} onChange={e => { setQuery(e.target.value); if (prefs.view === 'games' && e.target.value) setPrefs({ view: 'all' }); }} /></div>
        <div className="spacer" />
        <span className="muted small">{list.length === clips.length ? `${clips.length} clips` : `${list.length} of ${clips.length}`} · {bytes(total)}</span>
        <IconButton label="Open clips folder" icon={FolderOpen} onClick={() => run('Opening folder', api.openFolder)} />
        <IconButton label="Refresh" icon={RefreshCw} disabled={!!busy} onClick={() => run('Refreshing', refreshClips)} />
      </div>

      {prefs.view === 'all' && clips.length > 0 && (
        <div className="toolbar filters">
          <select aria-label="Game" value={prefs.game} onChange={e => setPrefs({ game: e.target.value })}>
            <option value={ALL}>All games</option>
            {cats.map(c => <option key={c.name} value={c.name}>{c.name} ({c.count})</option>)}
          </select>
          <Segmented label="Type" value={prefs.kind} onChange={v => setPrefs({ kind: v })}
            options={[{ value: 'all', label: 'All' }, { value: 'Replay', label: 'Replays' }, { value: 'Recording', label: 'Recordings' }, { value: 'Export', label: 'MP4' }]} />
          <label className="date-range" title="Only clips saved between these dates">
            <CalendarRange size={14} />
            <input type="date" aria-label="From date" value={prefs.from} max={prefs.to || undefined} onChange={e => setPrefs({ from: e.target.value })} />
            <span className="muted">to</span>
            <input type="date" aria-label="To date" value={prefs.to} min={prefs.from || undefined} onChange={e => setPrefs({ to: e.target.value })} />
          </label>
          <select aria-label="Sort" value={prefs.sort} onChange={e => setPrefs({ sort: e.target.value as Sort })}>
            {SORTS.map(s => <option key={s.value} value={s.value}>{s.label}</option>)}
          </select>
          {filtered && <Button size="sm" variant="ghost" icon={X} onClick={() => { setPrefs({ game: ALL, kind: 'all', from: '', to: '' }); setQuery(''); }}>Clear filters</Button>}
        </div>
      )}

      {pendingSaves.length > 0 && (
        <div className="pending-saves">
          {pendingSaves.map(save => (
            <div key={save.id} className={`pending ${save.stage === 'Failed' ? 'failed' : ''}`} role="status">
              {save.stage === 'Failed' ? <X size={15} /> : <LoaderCircle size={15} className="spin" />}
              <strong>{save.stage === 'Failed' ? 'Save failed' : `${save.stage}…`}</strong>
              <span className="muted small">{save.error ?? `${(save.elapsed_ms / 1000).toFixed(1)} s · appears here when it can be played`}</span>
            </div>
          ))}
        </div>
      )}

      {clips.length === 0 ? (
        <Card><Empty icon={FileVideo} title="Your library is empty">Enable Instant Replay and press save, or start a recording. Clips are stored in {draft.output_dir}.</Empty></Card>
      ) : prefs.view === 'games' ? (
        <div className="game-tiles" role="list" aria-label="Games with clips">
          {tiles.map(t => (
            <button key={t.name} role="listitem" className="game-tile" onClick={() => { setPrefs({ view: 'all', game: t.name }); setQuery(''); }}>
              <GameIcon name={t.name} size={72} />
              <strong title={t.name}>{t.name}</strong>
              <span>{t.count} {t.count === 1 ? 'clip' : 'clips'}{t.seconds > 0 ? ` · ${duration(t.seconds)} of video` : ''}</span>
            </button>
          ))}
        </div>
      ) : (
        <div className={`library ${clip ? 'with-detail' : ''}`}>
          {list.length === 0 ? (
            <Card><Empty icon={Search} title="No matching clips">Try another search, game, type or date.</Empty></Card>
          ) : (
            <div>
              {prefs.game !== ALL && (
                <div className="game-filter-head"><GameIcon name={prefs.game} size={28} /><strong>{prefs.game}</strong><span className="muted small">{list.length} {list.length === 1 ? 'clip' : 'clips'}</span>
                  <Button size="sm" variant="ghost" icon={X} onClick={() => setPrefs({ game: ALL })}>All games</Button></div>
              )}
              <div className="clip-grid">
                {list.slice(0, shown).map(c => (
                  <button key={c.id} className={`clip-card ${selected === c.id ? 'on' : ''}`} onClick={() => setSelected(c.id)} onDoubleClick={() => act(c, 'play', 'Opening clip')}>
                    <div className="clip-thumb">
                      <Thumb clip={c} />
                      <span className={`kind-tag ${c.kind.toLowerCase()}`}>{kindLabel(c.kind)}</span>
                      {c.probe && <span className="dur">{duration(c.probe.duration)}</span>}
                    </div>
                    <div className="clip-meta">
                      <div className="clip-game"><GameIcon name={c.category} size={18} /><strong title={c.name}>{gameLabel(c)}</strong></div>
                      <span>{new Date(c.modified * 1000).toLocaleDateString([], { dateStyle: 'medium' })} · {new Date(c.modified * 1000).toLocaleTimeString([], { timeStyle: 'short' })}{c.probe ? ` · ${duration(c.probe.duration)}` : ''}</span>
                      <span>{c.probe ? `${c.probe.width}×${c.probe.height}${c.probe.fps ? ` · ${Math.round(c.probe.fps)} FPS` : ''} · ${bytes(c.bytes)}` : `${bytes(c.bytes)} · reading details…`}</span>
                    </div>
                  </button>
                ))}
              </div>
              {shown < list.length && <div ref={sentinel} className="load-more muted small">Loading more clips…</div>}
            </div>
          )}

          {clip && (
            <aside className="detail" aria-label="Clip details">
              <div className="detail-head"><h2 title={clip.name}>{stem(clip.name)}</h2><IconButton label="Close details" icon={X} onClick={() => setSelected(null)} /></div>
              <button className="detail-thumb" onClick={() => act(clip, 'play', 'Opening clip')} aria-label="Play in default player"><Thumb key={clip.id} clip={clip} /><span className="play"><Play size={22} /></span></button>
              <div className="detail-game"><GameIcon name={clip.category} size={22} /><strong>{gameLabel(clip)}</strong><Badge tone={clip.kind === 'Recording' ? 'rec' : clip.kind === 'Replay' ? 'accent' : 'neutral'}>{kindLabel(clip.kind)}</Badge></div>
              <div className="detail-actions">
                <Button variant="primary" icon={Play} onClick={() => act(clip, 'play', 'Opening clip')}>Play</Button>
                <Button icon={FolderOpen} onClick={() => act(clip, 'folder', 'Opening folder')}>Show</Button>
                <Button icon={Pencil} onClick={() => { setRenaming(clip); setNewName(stem(clip.name)); }}>Rename</Button>
                <Button icon={Gamepad2} onClick={() => { setMoving(clip); setTarget(clip.category === 'Unsorted' ? '' : clip.category); }}>Change game</Button>
                {ext(clip.name).toLowerCase() === '.mkv' && <Button icon={Share2} disabled={!!busy} onClick={() => act(clip, 'mp4', 'Exporting MP4', 'MP4 created and verified: all tracks copied without re-encoding.')}>MP4</Button>}
                <Button variant="danger" icon={Trash2} onClick={() => { setDeleting(clip); setDeleteError(null); }}>Delete</Button>
              </div>
              <dl className="facts compact">
                {clip.probe ? <>
                  <div><dt>Duration</dt><dd>{duration(clip.probe.duration)}</dd></div>
                  <div><dt>Video</dt><dd>{clip.probe.video_codec.toUpperCase()} · {clip.probe.width}×{clip.probe.height}{clip.probe.fps ? ` · ${clip.probe.fps.toFixed(clip.probe.fps % 1 ? 2 : 0)} FPS` : ''}</dd></div>
                  <div><dt>Audio tracks</dt><dd>{clip.probe.audio_tracks || 'None'}</dd></div>
                </> : <div><dt>Details</dt><dd className="muted">Reading…</dd></div>}
                <div><dt>Size</dt><dd>{bytes(clip.bytes)}</dd></div>
                <div><dt>Saved</dt><dd>{new Date(clip.modified * 1000).toLocaleString()}</dd></div>
                <div><dt>Location</dt><dd className="mono small">{clip.id.includes('/') ? `${clip.category}\\` : ''}{clip.name}</dd></div>
              </dl>
              {!!clip.probe?.audio_titles.length && (
                <ol className="track-list">
                  {clip.probe.audio_titles.map((t, i) => <li key={i}><b>{i + 1}</b>{t || `Track ${i + 1}`}</li>)}
                </ol>
              )}
              <p className="muted small">Players usually play track 1 (the mix). Editors such as DaVinci Resolve or Premiere can import each track separately.</p>
            </aside>
          )}
        </div>
      )}

      {renaming && (
        <Modal title="Rename clip" onClose={() => setRenaming(null)} actions={<>
          <Button variant="ghost" onClick={() => setRenaming(null)}>Cancel</Button>
          <Button variant="primary" disabled={!newName.trim() || stem(renaming.name) === newName.trim()} onClick={async () => {
            const id = await run('Renaming', () => api.clipAction(renaming.id, 'rename', newName.trim()), 'Clip renamed.');
            if (id) { forgetThumbs(renaming.id); await refreshClips(); setSelected(id); setRenaming(null); }
          }}>Rename</Button>
        </>}>
          <div className="input-suffix"><input value={newName} onChange={e => setNewName(e.target.value)} aria-label="New name" maxLength={150} /><span>{ext(renaming.name)}</span></div>
          <p className="muted small">Letters, numbers, spaces and dashes work best. These aren't allowed: \ / : * ? " &lt; &gt; |</p>
        </Modal>
      )}

      {moving && (
        <Modal title="Change game" onClose={() => setMoving(null)} actions={<>
          <Button variant="ghost" onClick={() => setMoving(null)}>Cancel</Button>
          <Button variant="primary" disabled={!target.trim() || target.trim() === moving.category} onClick={async () => {
            const id = await run('Moving clip', () => api.clipAction(moving.id, 'move', target.trim()), `Moved to ${target.trim()}.`);
            if (id) { forgetThumbs(moving.id); dropClip(moving.id); await refreshClips(); setSelected(id); setPrefs({ game: ALL }); setMoving(null); }
          }}>Move</Button>
        </>}>
          <p className="muted small">Moves the file into that game's folder. The file name stays the same.</p>
          <input list="replayx-games" value={target} onChange={e => setTarget(e.target.value)} placeholder="Game name, e.g. Valorant" aria-label="Game" autoFocus />
          <datalist id="replayx-games">{cats.filter(c => c.name !== 'Unsorted').map(c => <option key={c.name} value={c.name} />)}</datalist>
          <div className="chip-row">{cats.filter(c => c.name !== 'Unsorted' && c.name !== moving.category).slice(0, 8).map(c => <button key={c.name} className="chip" onClick={() => setTarget(c.name)}>{c.name}</button>)}</div>
        </Modal>
      )}

      {deleting && (
        <Modal title="Delete this clip?" onClose={() => !deletingNow && setDeleting(null)} actions={<>
          <Button variant="ghost" disabled={deletingNow} onClick={() => setDeleting(null)}>Keep</Button>
          <Button variant="danger" icon={deletingNow ? LoaderCircle : Trash2} disabled={deletingNow} onClick={confirmDelete}>{deletingNow ? 'Moving…' : 'Move to Recycle Bin'}</Button>
        </>}>
          <p><strong>{deleting.name}</strong></p>
          <p className="muted small">{deleting.category === 'Unsorted' ? 'Clips folder' : `${deleting.category} folder`} · {bytes(deleting.bytes)}{deleting.probe ? ` · ${duration(deleting.probe.duration)}` : ''}</p>
          <p className="muted">The file goes to the Windows Recycle Bin, so you can still restore it.</p>
          {deleteError && <div className="banner bad" role="alert">{deleteError}</div>}
        </Modal>
      )}
    </Section>
  );
}
