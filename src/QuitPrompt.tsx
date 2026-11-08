import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, errorText, native } from './api';
import { useStatus } from './store';
import { Button, Modal } from './ui';

/** Shown when Quit (tray or close button) is requested while a recording or
 * a save is in progress. Quitting stops capture, finalizes the recording and
 * ends the encoder; it never discards saved or recoverable footage. */
export function QuitPrompt({ onNavigate }: { onNavigate: (page: string) => void }) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [quitting, setQuitting] = useState(false);
  const s = useStatus(x => ({ recording: x.recording, saving: x.saving }));
  useEffect(() => {
    if (!native) return;
    const a = listen('confirm-quit', () => { setError(null); setOpen(true); });
    const b = listen<string>('navigate', e => onNavigate(e.payload));
    return () => { void a.then(u => u()); void b.then(u => u()); };
  }, [onNavigate]);
  if (!open) return null;
  const quit = async () => {
    setQuitting(true); setError(null);
    try { await api.exitApp(); } catch (e) { setError(errorText(e)); setQuitting(false); }
  };
  return (
    <Modal title="Quit REPLAYX?" onClose={() => setOpen(false)} actions={<>
      <Button variant="ghost" onClick={() => setOpen(false)}>Keep running</Button>
      <Button variant="danger" disabled={s.saving > 0 || quitting} onClick={quit}>
        {s.saving > 0 ? 'Waiting for the save…' : quitting ? 'Finishing…' : s.recording ? 'Stop recording and quit' : 'Quit'}
      </Button>
    </>}>
      {s.recording && <p>A recording is in progress. Quitting stops it and saves everything recorded so far as a clip.</p>}
      {s.saving > 0 && <p>A clip is still being saved and checked. Quit becomes available as soon as it is finished.</p>}
      {!s.recording && s.saving === 0 && <p>Nothing is being recorded or saved any more. Instant Replay stops when REPLAYX quits.</p>}
      {error && <div className="banner bad">{error}</div>}
    </Modal>
  );
}
