import { useMemo, useState } from 'react';
import { FileVideo, FolderOpen, Pencil, Play, RefreshCw, Search, Share2, Trash2, X } from 'lucide-react';
import { api, type Clip } from '../api';
import { useApp } from '../context';
import { bytes, duration } from '../format';
import { Badge, Button, Card, Empty, IconButton, Modal, Segmented } from '../ui';
import { Section, Thumb, clipKind } from './shared';

type Filter = 'all' | 'Replay' | 'Recording' | 'Export';
const stem = (n: string) => n.replace(/\.(mkv|mp4)$/i, '');
const ext = (n: string) => n.match(/\.(mkv|mp4)$/i)?.[0] ?? '';

export function ClipsPage() {
  const { clips, refreshClips, run, busy, draft } = useApp();
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [sort, setSort] = useState<'new' | 'old' | 'long' | 'size'>('new');
  const [selected, setSelected] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<Clip | null>(null);
  const [newName, setNewName] = useState('');
  const [deleting, setDeleting] = useState<Clip | null>(null);

  const list = useMemo(() => {
    const q = query.trim().toLowerCase();
    return clips
      .filter(c => (filter === 'all' || clipKind(c.name) === filter) && (!q || c.name.toLowerCase().includes(q)))
      .sort((a, b) => sort === 'new' ? b.modified - a.modified : sort === 'old' ? a.modified - b.modified
        : sort === 'long' ? (b.probe?.duration ?? 0) - (a.probe?.duration ?? 0) : (b.probe?.bytes ?? 0) - (a.probe?.bytes ?? 0));
  }, [clips, query, filter, sort]);
  const total = clips.reduce((n, c) => n + (c.probe?.bytes ?? 0), 0);
  const clip = clips.find(c => c.name === selected) ?? null;

  const act = (c: Clip, op: 'play' | 'folder' | 'mp4', label: string, done?: string) =>
    run(label, async () => { const r = await api.clipAction(c.name, op); if (op === 'mp4') await refreshClips(); return r; }, done);

  return (
    <Section>
      <div className="toolbar">
        <div className="search"><Search size={15} /><input placeholder="Search clips" aria-label="Search clips" value={query} onChange={e => setQuery(e.target.value)} /></div>
        <Segmented label="Filter" value={filter} onChange={setFilter}
          options={[{ value: 'all', label: 'All' }, { value: 'Replay', label: 'Replays' }, { value: 'Recording', label: 'Recordings' }, { value: 'Export', label: 'MP4' }]} />
        <select aria-label="Sort" value={sort} onChange={e => setSort(e.target.value as typeof sort)}>
          <option value="new">Newest</option><option value="old">Oldest</option><option value="long">Longest</option><option value="size">Largest</option>
        </select>
        <div className="spacer" />
        <span className="muted small">{clips.length} clips · {bytes(total)}</span>
        <IconButton label="Open clips folder" icon={FolderOpen} onClick={() => run('Opening folder', api.openFolder)} />
        <IconButton label="Refresh" icon={RefreshCw} disabled={!!busy} onClick={() => run('Refreshing', refreshClips)} />
      </div>

      <div className={`library ${clip ? 'with-detail' : ''}`}>
        {clips.length === 0 ? (
          <Card><Empty icon={FileVideo} title="Your library is empty">Enable Instant Replay and press save, or start a recording. Clips are stored in {draft.output_dir}.</Empty></Card>
        ) : list.length === 0 ? (
          <Card><Empty icon={Search} title="No matching clips">Try another search or filter.</Empty></Card>
        ) : (
          <div className="clip-grid">
            {list.map(c => (
              <button key={c.name} className={`clip-card ${selected === c.name ? 'on' : ''}`} onClick={() => setSelected(c.name)} onDoubleClick={() => act(c, 'play', 'Opening clip')}>
                <div className="clip-thumb"><Thumb clip={c} />{c.probe && <span className="dur">{duration(c.probe.duration)}</span>}</div>
                <div className="clip-meta">
                  <div className="clip-title"><Badge tone={clipKind(c.name) === 'Recording' ? 'rec' : clipKind(c.name) === 'Replay' ? 'accent' : 'neutral'}>{clipKind(c.name)}</Badge><strong title={c.name}>{stem(c.name)}</strong></div>
                  <span>{new Date(c.modified * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })}</span>
                  <span>{c.probe ? `${c.probe.width}×${c.probe.height}${c.probe.fps ? ` · ${Math.round(c.probe.fps)} fps` : ''} · ${bytes(c.probe.bytes)}` : 'Unreadable file'}</span>
                </div>
              </button>
            ))}
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
              {ext(clip.name).toLowerCase() === '.mkv' && <Button icon={Share2} disabled={!!busy} onClick={() => act(clip, 'mp4', 'Exporting MP4', 'MP4 created and verified — all tracks copied without re-encoding.')}>MP4</Button>}
              <Button variant="danger" icon={Trash2} onClick={() => setDeleting(clip)}>Delete</Button>
            </div>
            {clip.probe ? (
              <dl className="facts compact">
                <div><dt>Duration</dt><dd>{duration(clip.probe.duration)}</dd></div>
                <div><dt>Video</dt><dd>{clip.probe.video_codec.toUpperCase()} · {clip.probe.width}×{clip.probe.height}{clip.probe.fps ? ` · ${clip.probe.fps.toFixed(clip.probe.fps % 1 ? 2 : 0)} fps` : ''}</dd></div>
                <div><dt>Size</dt><dd>{bytes(clip.probe.bytes)}</dd></div>
                <div><dt>Saved</dt><dd>{new Date(clip.modified * 1000).toLocaleString()}</dd></div>
                <div><dt>Audio tracks</dt><dd>{clip.probe.audio_tracks || 'None'}</dd></div>
              </dl>
            ) : <div className="banner warn">This file could not be read. It may be incomplete.</div>}
            {!!clip.probe?.audio_titles.length && (
              <ol className="track-list">
                {clip.probe.audio_titles.map((t, i) => <li key={i}><b>{i + 1}</b>{t || `Track ${i + 1}`}</li>)}
              </ol>
            )}
            <p className="muted small">Players usually play track 1 (the mix). Video editors such as DaVinci Resolve or Premiere can import each track separately.</p>
          </aside>
        )}
      </div>

      {renaming && (
        <Modal title="Rename clip" onClose={() => setRenaming(null)} actions={<>
          <Button variant="ghost" onClick={() => setRenaming(null)}>Cancel</Button>
          <Button variant="primary" disabled={!newName.trim() || stem(renaming.name) === newName.trim()} onClick={async () => {
            const target = newName.trim() + ext(renaming.name);
            const ok = await run('Renaming', () => api.clipAction(renaming.name, 'rename', target), 'Clip renamed.');
            if (ok !== undefined) { await refreshClips(); setSelected(target); setRenaming(null); }
          }}>Rename</Button>
        </>}>
          <div className="input-suffix"><input value={newName} onChange={e => setNewName(e.target.value)} aria-label="New name" maxLength={150} /><span>{ext(renaming.name)}</span></div>
          <p className="muted small">Letters, numbers, spaces and dashes work best. These aren't allowed: \ / : * ? " &lt; &gt; |</p>
        </Modal>
      )}
      {deleting && (
        <Modal title="Delete this clip?" onClose={() => setDeleting(null)} actions={<>
          <Button variant="ghost" onClick={() => setDeleting(null)}>Keep</Button>
          <Button variant="danger" icon={Trash2} onClick={async () => {
            const name = deleting.name;
            setDeleting(null);
            const ok = await run('Deleting', () => api.clipAction(name, 'delete'), 'Moved to the Recycle Bin.');
            if (ok !== undefined) { if (selected === name) setSelected(null); await refreshClips(); }
          }}>Move to Recycle Bin</Button>
        </>}>
          <p><strong>{deleting.name}</strong></p>
          <p className="muted">The file goes to the Windows Recycle Bin, so you can still restore it.</p>
        </Modal>
      )}
    </Section>
  );
}
