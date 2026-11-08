import React, { Component, useEffect, useId, useRef } from 'react';
import { X } from 'lucide-react';

export function Card({ title, actions, children, className = '', subtitle, tour }: {
  title?: React.ReactNode; subtitle?: React.ReactNode; actions?: React.ReactNode; children: React.ReactNode; className?: string;
  /** Anchor for the first-run tour (data-tour). */
  tour?: string;
}) {
  return (
    <section className={`card ${className}`} data-tour={tour}>
      {(title || actions) && (
        <header className="card-head">
          <div>{title && <h2>{title}</h2>}{subtitle && <p className="card-sub">{subtitle}</p>}</div>
          {actions && <div className="card-actions">{actions}</div>}
        </header>
      )}
      {children}
    </section>
  );
}

export function Button({ variant = 'default', icon: Icon, children, size, ...rest }: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'default' | 'primary' | 'danger' | 'ghost' | 'record'; icon?: React.ComponentType<{ size?: number }>; size?: 'sm' | 'lg';
}) {
  return (
    <button type="button" {...rest} className={`btn btn-${variant} ${size ? `btn-${size}` : ''} ${rest.className ?? ''}`}>
      {Icon && <Icon size={size === 'lg' ? 18 : 15} />}
      {children && <span>{children}</span>}
    </button>
  );
}

export function IconButton({ label, icon: Icon, ...rest }: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string; icon: React.ComponentType<{ size?: number }>;
}) {
  return <button type="button" aria-label={label} title={label} {...rest} className={`icon-btn ${rest.className ?? ''}`}><Icon size={16} /></button>;
}

export function Toggle({ checked, onChange, label, description, disabled }: {
  checked: boolean; onChange: (v: boolean) => void; label: React.ReactNode; description?: React.ReactNode; disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className={`toggle-row ${disabled ? 'disabled' : ''}`}>
      <div className="toggle-text"><label htmlFor={id}>{label}</label>{description && <small>{description}</small>}</div>
      <button id={id} type="button" role="switch" aria-checked={checked} disabled={disabled}
        className={`switch ${checked ? 'on' : ''}`} onClick={() => onChange(!checked)}><span /></button>
    </div>
  );
}

export function Field({ label, hint, children, wide }: { label: React.ReactNode; hint?: React.ReactNode; children: React.ReactNode; wide?: boolean }) {
  return <label className={`field ${wide ? 'wide' : ''}`}><span className="field-label">{label}</span>{children}{hint && <small className="field-hint">{hint}</small>}</label>;
}

export function Segmented<T extends string | number>({ value, options, onChange, disabled, label }: {
  value: T; options: { value: T; label: React.ReactNode; hint?: string; disabled?: boolean }[]; onChange: (v: T) => void; disabled?: boolean; label: string;
}) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map(o => (
        <button key={String(o.value)} type="button" role="radio" aria-checked={o.value === value} title={o.hint}
          disabled={disabled || o.disabled} className={o.value === value ? 'on' : ''} onClick={() => onChange(o.value)}>{o.label}</button>
      ))}
    </div>
  );
}

export function Badge({ tone = 'neutral', children, dot }: { tone?: 'neutral' | 'ok' | 'warn' | 'bad' | 'accent' | 'rec'; children: React.ReactNode; dot?: boolean }) {
  return <span className={`badge badge-${tone}`}>{dot && <i />}{children}</span>;
}

/** Peak meter from a real linear peak (0..1+); dB scale, -60..0 dBFS. */
export function Meter({ peak, active, vertical }: { peak: number; active: boolean; vertical?: boolean }) {
  const dbfs = peak > 0 ? 20 * Math.log10(peak) : -100;
  const level = active ? Math.max(0, Math.min(1, (dbfs + 60) / 60)) : 0;
  return (
    <div className={`meter ${vertical ? 'vertical' : ''} ${active ? '' : 'idle'}`} role="meter" aria-valuemin={-60} aria-valuemax={0}
      aria-valuenow={active ? Math.round(Math.max(-60, dbfs)) : undefined} aria-label={active ? 'Audio level' : 'Audio level unavailable while capture is off'}>
      <span style={{ [vertical ? 'height' : 'width']: `${level * 100}%` }} className={dbfs > -3 ? 'hot' : dbfs > -12 ? 'warm' : ''} />
    </div>
  );
}

export function Modal({ title, children, onClose, actions }: { title: string; children: React.ReactNode; onClose: () => void; actions: React.ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const first = ref.current?.querySelector<HTMLElement>('input, button.btn-primary, button.btn-danger, button');
    first?.focus();
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', key);
    return () => { window.removeEventListener('keydown', key); previous?.focus(); };
  }, [onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={title} ref={ref}>
        <header><h2>{title}</h2><IconButton label="Close" icon={X} onClick={onClose} /></header>
        <div className="modal-body">{children}</div>
        <footer>{actions}</footer>
      </div>
    </div>
  );
}

export function Empty({ icon: Icon, title, children }: { icon: React.ComponentType<{ size?: number }>; title: string; children?: React.ReactNode }) {
  return <div className="empty"><div className="empty-icon"><Icon size={22} /></div><h3>{title}</h3>{children && <p>{children}</p>}</div>;
}

export function Kbd({ combo }: { combo: string }) {
  return <span className="kbd">{combo.split('+').map((k, i) => <kbd key={i}>{k === 'Control' ? 'Ctrl' : k}</kbd>)}</span>;
}

export function Stat({ label, value, detail, tone }: { label: string; value: React.ReactNode; detail?: React.ReactNode; tone?: 'ok' | 'warn' | 'bad' }) {
  return <div className={`stat ${tone ? `stat-${tone}` : ''}`}><span className="stat-label">{label}</span><strong className="stat-value">{value}</strong>{detail && <small>{detail}</small>}</div>;
}

export function Sparkline({ values, max, label }: { values: number[]; max?: number; label: string }) {
  if (values.length < 2) return <div className="sparkline empty-line" aria-label={`${label}: collecting samples`} />;
  const top = Math.max(max ?? 0, ...values, 1e-9);
  const pts = values.map((v, i) => `${(i / (values.length - 1)) * 100},${30 - (v / top) * 28}`).join(' ');
  return (
    <svg className="sparkline" viewBox="0 0 100 30" preserveAspectRatio="none" role="img" aria-label={label}>
      <polyline points={`0,30 ${pts} 100,30`} className="area" />
      <polyline points={pts} className="line" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

/** Keeps a failing page from blanking the whole window. */
export class PageBoundary extends Component<{ children: React.ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) { return { error }; }
  componentDidCatch(error: Error) { console.error('REPLAYX page error:', error); }
  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="page"><div className="banner bad" role="alert">
        <span>This page hit an unexpected error: {this.state.error.message}. Capture is not affected.</span>
        <button className="btn btn-sm" onClick={() => this.setState({ error: null })}>Try again</button>
      </div></div>
    );
  }
}
