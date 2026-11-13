import { useRef, useState } from 'react';
import { createGuard } from './retry';
import { AlertTriangle, ChevronRight, X } from 'lucide-react';
import { api, native } from './api';
import { useCapture } from './capture';
import type { ErrorAction, FriendlyError } from './errors';
import { navigateTo } from './navigation';

/** One error, the same everywhere (home banner, toast): a plain title and
 * sentence, actions that lead to the fix, and the technical text folded
 * away for diagnostics.
 *
 * Retry repeats exactly the operations that failed (`ops`). While it runs,
 * every action here is disabled, so a double click can't start capture
 * twice. `onAction(true)` follows an action that worked; a Retry that fails
 * again reports `false`, and the notice should stay. */
export function ErrorNotice({ error, count = 1, onDismiss, onAction }: {
  error: FriendlyError; count?: number; onDismiss?: () => void; onAction?: (ok: boolean) => void;
}) {
  const { retry, busy } = useCapture();
  const [working, setWorking] = useState(false);
  const guard = useRef(createGuard());
  const run = (a: ErrorAction) => guard.current.run(async () => {
    setWorking(true);
    let ok = true;
    try {
      switch (a.kind) {
        case 'navigate': navigateTo(a.to); break;
        case 'windows': if (native) await api.openWindowsSettings(a.page).catch(() => undefined); break;
        case 'retry': ok = await retry(a.ops); break;
        case 'folder': if (native) await api.openFolder().catch(() => undefined); break;
      }
    } finally {
      setWorking(false);
    }
    onAction?.(ok);
  });
  // Another capture action is already running: Retry would conflict with it.
  const blocked = (a: ErrorAction) => working || (a.kind === 'retry' && !!busy);
  const primary = error.actions.find(a => a.kind === 'navigate');
  return (
    <div className={`error-notice ${error.severity}`} role={error.severity === 'error' ? 'alert' : 'status'}>
      <span className="en-icon" aria-hidden><AlertTriangle size={16} /></span>
      <div className="en-body">
        {/* The title leads to the setting that fixes it (also by keyboard). */}
        {primary
          ? <button type="button" className="en-title" disabled={working} onClick={() => run(primary)}>{error.title}{count > 1 && <span className="en-count">×{count}</span>}<ChevronRight size={14} aria-hidden /></button>
          : <strong className="en-title">{error.title}{count > 1 && <span className="en-count">×{count}</span>}</strong>}
        <p>{error.message}</p>
        {error.actions.length > 0 && (
          <div className="en-actions">
            {error.actions.map((a, i) => (
              <button type="button" key={a.label} className={i === 0 ? 'en-primary' : ''} disabled={blocked(a)} aria-busy={working && a.kind === 'retry'} onClick={() => run(a)}>
                {a.kind === 'retry' && working ? 'Retrying…' : a.label}{a.kind === 'navigate' && <ChevronRight size={13} aria-hidden />}
              </button>
            ))}
          </div>
        )}
        <details className="en-details">
          <summary>Technical details</summary>
          <code>{error.technical}</code>
        </details>
      </div>
      {onDismiss && <button type="button" className="en-close" aria-label="Dismiss" onClick={onDismiss}><X size={14} /></button>}
    </div>
  );
}
