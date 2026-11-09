import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ArrowLeft, Clock, Film, Gamepad2, ImagePlus, Pause, Play, RotateCcw, ShieldCheck } from 'lucide-react';
import { api, errorText, native, type PlaySession, type PlaytimeData } from '../api';
import { useApp } from '../context';
import { duration } from '../format';
import { RANGES, dailySeries, dayKey, hours, range, totals, type RangeKind } from '../playtime';
import { Badge, Button, Card, Empty, Segmented, Toggle } from '../ui';
import { Section, Thumb } from './shared';
import { GameIcon, refreshArt } from '../gameArt';
import { showGameClips } from './Clips';

const SPECIAL = new Set(['Desktop', 'Mixed', 'Unsorted']);
const dayLabel = (k: string, long = false) => {
  const [y, m, d] = k.split('-').map(Number);
  return new Date(y, m - 1, d).toLocaleDateString([], long ? { weekday: 'short', month: 'short', day: 'numeric' } : { month: 'short', day: 'numeric' });
};

/** Daily playtime bars: one series (violet), hover for exact values. */
function DailyChart({ series, label }: { series: { day: string; seconds: number }[]; label: string }) {
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(...series.map(d => d.seconds), 1);
  const top = Math.max(1, Math.ceil(max / 3600)) * 3600;
  const W = 640, H = 168, pad = { l: 34, r: 6, t: 8, b: 22 };
  const step = (W - pad.l - pad.r) / Math.max(series.length, 1);
  const bw = Math.max(2, Math.min(22, step - 2));
  const y = (s: number) => pad.t + (H - pad.t - pad.b) * (1 - s / top);
  const labelEvery = Math.ceil(series.length / 7);
  if (!series.some(d => d.seconds > 0)) return <p className="muted small chart-empty">No playtime in this period yet.</p>;
  const h = hover !== null ? series[hover] : null;
  return (
    <div className="chart" aria-label={label}>
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${label}: ${series.map(d => `${dayLabel(d.day)} ${hours(d.seconds)}`).join(', ')}`} onMouseLeave={() => setHover(null)}>
        {[0, 0.5, 1].map(f => (
          <g key={f}>
            <line x1={pad.l} x2={W - pad.r} y1={y(top * f)} y2={y(top * f)} className="grid" />
            <text x={pad.l - 6} y={y(top * f) + 3.5} className="axis" textAnchor="end">{f === 0 ? '0' : `${+((top * f) / 3600).toFixed(1)}h`}</text>
          </g>
        ))}
        {series.map((d, i) => {
          const x = pad.l + i * step + (step - bw) / 2;
          const bh = Math.max(d.seconds > 0 ? 2 : 0, H - pad.b - y(d.seconds));
          return (
            <g key={d.day} onMouseEnter={() => setHover(i)}>
              <rect x={pad.l + i * step} y={pad.t} width={step} height={H - pad.t - pad.b} fill="transparent" />
              <path className={`bar ${hover === i ? 'on' : ''}`} d={`M${x},${H - pad.b} v${-Math.max(0, bh - 3)} q0,-3 3,-3 h${bw - 6} q3,0 3,3 v${Math.max(0, bh - 3)} z`} />
              {i % labelEvery === 0 && <text x={x + bw / 2} y={H - 6} className="axis" textAnchor="middle">{dayLabel(d.day)}</text>}
            </g>
          );
        })}
      </svg>
      {h && hover !== null && (
        <div className="chart-tip" style={{ left: `${((pad.l + hover * step + step / 2) / W) * 100}%` }}>
          <strong>{hours(h.seconds)}</strong><span>{dayLabel(h.day, true)}</span>
        </div>
      )}
    </div>
  );
}

function OptIn({ onEnable }: { onEnable: () => void }) {
  return (
    <Card className="optin" title="Track how long you play" subtitle="Optional, and entirely on this PC.">
      <ul className="optin-list">
        <li><Clock size={15} />Counts time while a game is the window in front and REPLAYX is running. A game open in the background doesn't count.</li>
        <li><Pause size={15} />Stops counting after 15 minutes without keyboard or mouse input (adjustable in Settings), and never counts sleep.</li>
        <li><ShieldCheck size={15} />Stored only on this PC. No account, no uploads. Pause, exclude games, export or clear it anytime in Settings.</li>
      </ul>
      <p className="muted small">Nothing from before you turn this on is counted, and REPLAYX does not guess playtime from your clips.</p>
      <div className="row"><Button variant="primary" icon={Play} disabled={!native} onClick={onEnable}>Start tracking playtime</Button></div>
    </Card>
  );
}

export function GamesPage({ go }: { go: (page: 'clips' | 'settings') => void }) {
  const { clips, run } = useApp();
  const [data, setData] = useState<PlaytimeData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [kind, setKind] = useState<RangeKind>('last7');
  const [custom, setCustom] = useState({ from: dayKey(new Date(Date.now() - 6 * 864e5)), to: dayKey(new Date()) });
  const [game, setGame] = useState<string | null>(null);
  const load = useCallback(() => { if (native) api.playtimeData().then(d => { setData(d); setError(null); }).catch(e => setError(errorText(e))); }, []);
  useEffect(() => { load(); const t = window.setInterval(load, 10_000); return () => window.clearInterval(t); }, [load]);
  const settings = data?.settings;
  const configure = (p: Partial<NonNullable<typeof settings>>) => settings && run('Saving', async () => { await api.playtimeConfigure({ ...settings, ...p }); load(); });

  // Include the session being tracked right now.
  const sessions: PlaySession[] = useMemo(() => {
    const all = data?.sessions ?? [];
    return data?.current ? [...all, { id: 'current', game: data.current.game, start: data.current.start, seconds: data.current.seconds, source: 'live' }] : all;
  }, [data]);
  const now = new Date();
  const r = range(kind, now, custom);
  const from = kind === 'all' ? Math.min(r.to - 60 * 864e5, ...sessions.map(s => s.start)) : r.from;
  const period = useMemo(() => totals(sessions, r.from, r.to), [sessions, r.from, r.to]);
  const series = useMemo(() => dailySeries(sessions, Math.max(from, r.to - 120 * 864e5), r.to), [sessions, from, r.to]);
  const clipGames = useMemo(() => {
    const m = new Map<string, { count: number; seconds: number }>();
    for (const c of clips) if (!SPECIAL.has(c.category)) { const g = m.get(c.category) ?? { count: 0, seconds: 0 }; g.count++; g.seconds += c.probe?.duration ?? 0; m.set(c.category, g); }
    return m;
  }, [clips]);
  const allTime = useMemo(() => totals(sessions, 0, Date.now() + 864e5), [sessions]);
  const names = useMemo(() => {
    const set = new Set<string>([...allTime.games.map(g => g.game), ...clipGames.keys()]);
    return [...set].sort((a, b) => (period.games.find(g => g.game === b)?.seconds ?? 0) - (period.games.find(g => g.game === a)?.seconds ?? 0) || (clipGames.get(b)?.count ?? 0) - (clipGames.get(a)?.count ?? 0) || a.localeCompare(b));
  }, [allTime, clipGames, period]);

  if (game) return <GameDetail game={game} sessions={sessions} back={() => setGame(null)} go={go} excluded={!!settings?.excluded.some(g => g.toLowerCase() === game.toLowerCase())}
    onExclude={v => settings && configure({ excluded: v ? [...settings.excluded, game] : settings.excluded.filter(g => g.toLowerCase() !== game.toLowerCase()) })} />;

  const top = period.games.slice(0, 6);
  const maxTop = Math.max(...top.map(g => g.seconds), 1);
  return (
    <Section>
      {error && <div className="banner bad">{error}</div>}
      {settings && !settings.enabled && <OptIn onEnable={() => configure({ enabled: true, paused: false })} />}
      {settings?.enabled && (
        <div className="tracking-bar">
          <span className={`dot ${settings.paused ? 'paused' : 'on'}`} />
          {settings.paused ? 'Playtime tracking is paused.' : data?.current ? <>Tracking <strong>{data.current.game}</strong> · {hours(data.current.seconds)} this session</> : 'Tracking foreground playtime while REPLAYX runs.'}
          <span className="spacer" />
          <Button size="sm" variant="ghost" icon={settings.paused ? Play : Pause} onClick={() => configure({ paused: !settings.paused })}>{settings.paused ? 'Resume' : 'Pause'}</Button>
          <Button size="sm" variant="ghost" onClick={() => go('settings')}>Privacy settings</Button>
        </div>
      )}

      <div className="toolbar filters">
        <Segmented label="Period" value={kind} onChange={setKind} options={RANGES} />
        {kind === 'custom' && (
          <label className="date-range">
            <input type="date" aria-label="From date" value={custom.from} onChange={e => setCustom(c => ({ ...c, from: e.target.value }))} />
            <span className="muted">to</span>
            <input type="date" aria-label="To date" value={custom.to} onChange={e => setCustom(c => ({ ...c, to: e.target.value }))} />
          </label>
        )}
      </div>

      <div className="grid-2 games-overview">
        <Card title="Playtime" subtitle="Tracked foreground time in the selected period.">
          <div className="hero-number">{hours(period.total)}</div>
          <DailyChart series={series} label="Daily playtime" />
        </Card>
        <Card title="Most played">
          {top.length === 0 ? <p className="muted small">{settings?.enabled ? 'Play a game with REPLAYX running to see it here.' : 'Turn on playtime tracking to see your most played games.'}</p> : (
            <ol className="rank">
              {top.map(g => (
                <li key={g.game}><button onClick={() => setGame(g.game)}>
                  <GameIcon name={g.game} size={26} />
                  <span className="rank-name">{g.game}</span>
                  <span className="rank-bar"><i style={{ width: `${(g.seconds / maxTop) * 100}%` }} /></span>
                  <span className="rank-value">{hours(g.seconds)}</span>
                </button></li>
              ))}
            </ol>
          )}
        </Card>
      </div>

      <Card title="Your games" subtitle="Games you played with tracking on, or have clips of.">
        {names.length === 0 ? <Empty icon={Gamepad2} title="No games yet">Games appear here once you play with REPLAYX running, or save a clip of one.</Empty> : (
          <div className="game-tiles">
            {names.map(n => {
              const p = period.games.find(g => g.game === n);
              const c = clipGames.get(n);
              return (
                <button key={n} className="game-tile" onClick={() => setGame(n)}>
                  <GameIcon name={n} size={64} />
                  <strong title={n}>{n}</strong>
                  <span>{p ? hours(p.seconds) : '—'}{c ? ` · ${c.count} ${c.count === 1 ? 'clip' : 'clips'}` : ''}</span>
                </button>
              );
            })}
          </div>
        )}
      </Card>
    </Section>
  );
}

function GameDetail({ game, sessions, back, go, excluded, onExclude }: {
  game: string; sessions: PlaySession[]; back: () => void; go: (p: 'clips' | 'settings') => void; excluded: boolean; onExclude: (v: boolean) => void;
}) {
  const { clips, run } = useApp();
  const file = useRef<HTMLInputElement>(null);
  const mine = useMemo(() => sessions.filter(s => s.game === game), [sessions, game]);
  const now = new Date();
  const t = (k: RangeKind) => { const r = range(k, now); return totals(mine, r.from, r.to).total; };
  const all = totals(mine, 0, Date.now() + 864e5);
  const last = all.games[0]?.last;
  const gameClips = useMemo(() => clips.filter(c => c.category === game).sort((a, b) => b.modified - a.modified), [clips, game]);
  const clipSeconds = gameClips.reduce((n, c) => n + (c.probe?.duration ?? 0), 0);
  const r30 = range('last30', now);
  const series = useMemo(() => dailySeries(mine, r30.from, r30.to), [mine, r30.from, r30.to]);
  const recent = [...mine].sort((a, b) => b.start - a.start).slice(0, 8);
  const pick = (f: File) => {
    const reader = new FileReader();
    reader.onload = () => run('Saving image', async () => { await api.gameArtSet(game, String(reader.result)); refreshArt(game); });
    reader.readAsDataURL(f);
  };
  const stats: [string, string][] = [
    ['Total tracked', hours(all.total)], ['Today', hours(t('today'))], ['Yesterday', hours(t('yesterday'))],
    ['This week', hours(t('week'))], ['This month', hours(t('month'))],
    ['Last played', last ? new Date(last).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' }) : '—'],
    ['Clips', String(gameClips.length)], ['Clip footage', clipSeconds ? duration(clipSeconds) : '—'],
  ];
  return (
    <Section>
      <div className="game-head">
        <Button variant="ghost" icon={ArrowLeft} onClick={back}>Games</Button>
        <GameIcon name={game} size={56} />
        <div><h2>{game}</h2><span className="muted small">Tracked foreground playtime while REPLAYX was running.</span></div>
        <span className="spacer" />
        <input ref={file} type="file" accept="image/png,image/jpeg,image/webp" hidden onChange={e => { const f = e.target.files?.[0]; if (f) pick(f); e.target.value = ''; }} />
        <Button size="sm" icon={ImagePlus} disabled={!native} onClick={() => file.current?.click()}>Choose image</Button>
        <Button size="sm" variant="ghost" icon={RotateCcw} disabled={!native} onClick={() => run('Resetting image', async () => { await api.gameArtClear(game); refreshArt(game); })}>Default image</Button>
      </div>
      <div className="stat-grid">
        {stats.map(([k, v]) => <div key={k} className="stat"><small>{k}</small><strong>{v}</strong></div>)}
      </div>
      <div className="grid-2">
        <Card title="Last 30 days"><DailyChart series={series} label={`${game} daily playtime`} /></Card>
        <Card title="Recent sessions">
          {recent.length === 0 ? <p className="muted small">No tracked sessions yet.</p> : (
            <ul className="sessions">
              {recent.map(s => (
                <li key={s.id}><span>{new Date(s.start).toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' })}</span>
                  <span className="muted">{new Date(s.start).toLocaleTimeString([], { timeStyle: 'short' })}</span>
                  <strong>{hours(s.seconds)}</strong>{s.id === 'current' && <Badge tone="ok" dot>Now</Badge>}</li>
              ))}
            </ul>
          )}
        </Card>
      </div>
      <Card title="Recent clips" actions={gameClips.length > 0 && <Button size="sm" variant="ghost" icon={Film} onClick={() => { showGameClips(game); go('clips'); }}>All {gameClips.length} clips</Button>}>
        {gameClips.length === 0 ? <p className="muted small">No clips of {game} yet.</p> : (
          <div className="mini-clips">
            {gameClips.slice(0, 4).map(c => (
              <button key={c.id} className="mini-clip" onClick={() => { showGameClips(game); go('clips'); }}>
                <Thumb clip={c} /><span>{new Date(c.modified * 1000).toLocaleDateString([], { dateStyle: 'medium' })}{c.probe ? ` · ${duration(c.probe.duration)}` : ''}</span>
              </button>
            ))}
          </div>
        )}
      </Card>
      <Card title="Tracking">
        <Toggle label="Track this game" checked={!excluded} onChange={v => onExclude(!v)}
          description="Turn off to never record playtime for this game. Already tracked time stays until you clear the history in Settings." />
      </Card>
    </Section>
  );
}
