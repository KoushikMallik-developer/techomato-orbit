import type { ReactNode } from 'react';

type Tone = 'neutral' | 'running' | 'warn' | 'danger';
const TONES: Record<Tone, string> = {
  neutral: 'bg-raised text-fg-muted',
  running: 'bg-running-soft text-running',
  warn: 'bg-warn-soft text-warn',
  danger: 'bg-danger-soft text-danger',
};
const DOTS: Record<Tone, string> = {
  neutral: 'bg-stopped-dot',
  running: 'bg-running-dot',
  warn: 'bg-warn-dot',
  danger: 'bg-danger-dot',
};

export function Pill({
  children,
  tone,
  dot = false,
}: {
  children: ReactNode;
  tone?: Tone;
  dot?: boolean;
}) {
  const t: Tone = tone ?? 'neutral';
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-semibold ${TONES[t]}`}
    >
      {dot && <span aria-hidden className={`size-1.5 rounded-full ${DOTS[t]}`} />}
      {children}
    </span>
  );
}
