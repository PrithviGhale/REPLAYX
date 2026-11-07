import { useEffect, useMemo, useRef, useState } from 'react';
import { FileVideo, FolderOpen, Gamepad2, LoaderCircle, Pencil, Play, RefreshCw, Search, Share2, Trash2, X } from 'lucide-react';
import { api, errorText, type Clip } from '../api';
import { useApp } from '../context';
import { bytes, duration } from '../format';
import { ALL, categories, filterClips, nextSelection, type Sort } from '../library';
import { Badge, Button, Card, Empty, IconButton, Modal, Segmented } from '../ui';
import { Section, Thumb } from './shared';

const stem = (n: string) => n.replace(/\.(mkv|mp4)$/i, '');
const ext = (n: string) => n.match(/\.(mkv|mp4)$/i)?.[0] ?? '';
const PAGE = 60;

export function ClipsPage() {
  const { clips, refreshClips, dropClip, pendingSaves, run, busy, draft, toast } = useApp();
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState(ALL);
  const [kind, setKind] = useState('all');
  const [sort, setSort] = useState<Sort>('new');
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
  const list = useMemo(() => filterClips(clips, category, kind, query, sort), [clips, category, kind, query, sort]);
  const total = useMemo(() => clips.reduce((n, c) => n + c.bytes, 0), [clips]);
  const clip = clips.find(c => c.id === selected) ?? null;
  useEffect(() => setShown(PAGE), [category, kind, query, sort]);
  useEffect(() => { if (category !== ALL && !cats.some(c => c.name === category)) setCategory(ALL); }, [cats, category]);
  // Render large libraries incrementally as the user scrolls.
  useEffect(() => {
    const el = sentinel.current;
    if (!el) return;
    const io = new IntersectionObserver(e => { if (e.some(x => x.isIntersecting)) setShown(n => n + PAGE); }, { rootMargin: '600px' });
    io.observe(el);
    return () => io.disconnect();
  }, [list.length, shown]);

  const act = (c: Clip, op: 'play' | 'folder' | 'mp4', label: string, done?: string) =>
    run(label, async () => { const r = await api.clipAction(c.id, op); if (op === 'mp4') await refreshClips(); return r; }, done);

  const confirmDelete = async () => {
    if (!deleting) return;
    setDeletingNow(true); setDeleteError(null);
    try {
      await api.clipAction(deleting.id, 'delete');
      const next = nextSelection(list, deleting.id);
      dropClip(deleting.id);
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

  return (
    <Section>
      <div className="category-rail" role="tablist" aria-label="Games">
        <button role="tab" aria-selected={category === ALL} className={category === ALL ? 'on' : ''} onClick={() => setCategory(ALL)}>All clips <small>{clips.length}</small></button>
        {cats.map(c => (
          <button key={c.name} role="tab" aria-selected={category === c.name} className={category === c.name ? 'on' : ''} onClick={() => setCategory(c.name)}>
            {c.name === 'Desktop' || c.name === 'Unsorted' || c.name === 'Mixed' ? null : <Gamepad2 size={13} />}{c.name} <small>{c.count}</small>
          </button>
        ))}
      </div>

      <div className="toolbar">
        <div className="search"><Search size={15} /><input placeholder="Search by name or game" aria-label="Search clips" value={query} onChange={e => setQuery(e.target.value)} /></div>
        <Segmented label="Type" value={kind} onChange={setKind}
          options={[{ value: 'all', label: 'All' }, { value: 'Replay', label: 'Replays' }, { value: 'Recording', label: 'Recordings' }, { value: 'Export', label: 'MP4' }]} />
        <select aria-label="Sort" value={sort} onChange={e => setSort(e.target.value as Sort)}>
          <option value="new">Newest</option><option value="old">Oldest</option><option value="long">Longest</option><option value="size">Largest</option>
        </select>
        <div className="spacer" />
        <span className="muted small">{list.length === clips.length ? `${clips.length} clips` : `${list.length} of ${clips.length}`} · {bytes(total)}</span>
        <IconButton label="Open clips folder" icon={FolderOpen} onClick={() => run('Opening folder', api.openFolder)} />
        <IconButton label="Refresh" icon={RefreshCw} disabled={!!busy} onClick={() => run('Refreshing', refreshClips)} />
      </div>

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

      <div className={`library ${clip ? 'with-detail' : ''}`}>
        {clips.length === 0 ? (
          <Card><Empty icon={FileVideo} title="Your library is empty">Enable Instant Replay and press save, or start a recording. Clips are stored in {draft.output_dir}.</Empty></Card>
        ) : list.length === 0 ? (
          <Card><Empty icon={Search} title="No matching clips">Try another search, type or game.</Empty></Card>
        ) : (
          <div>
            <div className="clip-grid">
              {list.slice(0, shown).map(c => (
                <button key={c.id} className={`clip-card ${selected === c.id ? 'on' : ''}`} onClick={() => setSelected(c.id)} onDoubleClick={() => act(c, 'play', 'Opening clip')}>
                  <div className="clip-thumb"><Thumb clip={c} />{c.probe && <span className="dur">{duration(c.probe.duration)}</span>}</div>
                  <div className="clip-meta">
                    <div className="clip-title">
                      <Badge tone={c.kind === 'Recording' ? 'rec' : c.kind === 'Replay' ? 'accent' : 'neutral'}>{c.kind}</Badge>
                      <strong title={c.name}>{c.category === 'Unsorted' ? stem(c.name) : c.category}</strong>
                    </div>
                    <span>{new Date(c.modified * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })}</span>
                    <span>{c.probe ? `${c.probe.width}×${c.probe.height}${c.probe.fps ? ` · ${Math.round(c.probe.fps)} fps` : ''} · ${bytes(c.bytes)}` : `${bytes(c.bytes)} · reading details…`}</span>
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
            <button className="detail-thumb" onClick={() => act(clip, 'play', 'Opening clip')} aria-label="Play in default player"><Thumb clip={clip} /><span className="play"><Play size={22} /></span></button>
            <div className="detail-actions">
              <Button variant="primary" icon={Play} onClick={() => act(clip, 'play', 'Opening clip')}>Play</Button>
              <Button icon={FolderOpen} onClick={() => act(clip, 'folder', 'Opening folder')}>Show</Button>
              <Button icon={Pencil} onClick={() => { setRenaming(clip); setNewName(stem(clip.name)); }}>Rename</Button>
              <Button icon={Gamepad2} onClick={() => { setMoving(clip); setTarget(clip.category === 'Unsorted' ? '' : clip.category); }}>Change game</Button>
              {ext(clip.name).toLowerCase() === '.mkv' && <Button icon={Share2} disabled={!!busy} onClick={() => act(clip, 'mp4', 'Exporting MP4', 'MP4 created and verified: all tracks copied without re-encoding.')}>MP4</Button>}
              <Button variant="danger" icon={Trash2} onClick={() => { setDeleting(clip); setDeleteError(null); }}>Delete</Button>
            </div>
            <dl className="facts compact">
              <div><dt>Game</dt><dd>{clip.category === 'Unsorted' ? (clip.probe?.game ?? 'Not recorded (older clip)') : clip.category}</dd></div>
              <div><dt>Type</dt><dd>{clip.kind}</dd></div>
              {clip.probe ? <>
                <div><dt>Duration</dt><dd>{duration(clip.probe.duration)}</dd></div>
                <div><dt>Video</dt><dd>{clip.probe.video_codec.toUpperCase()} · {clip.probe.width}×{clip.probe.height}{clip.probe.fps ? ` · ${clip.probe.fps.toFixed(clip.probe.fps % 1 ? 2 : 0)} fps` : ''}</dd></div>
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

      {renaming && (
        <Modal title="Rename clip" onClose={() => setRenaming(null)} actions={<>
          <Button variant="ghost" onClick={() => setRenaming(null)}>Cancel</Button>
          <Button variant="primary" disabled={!newName.trim() || stem(renaming.name) === newName.trim()} onClick={async () => {
            const id = await run('Renaming', () => api.clipAction(renaming.id, 'rename', newName.trim()), 'Clip renamed.');
            if (id) { await refreshClips(); setSelected(id); setRenaming(null); }
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
            if (id) { dropClip(moving.id); await refreshClips(); setSelected(id); setCategory(ALL); setMoving(null); }
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
