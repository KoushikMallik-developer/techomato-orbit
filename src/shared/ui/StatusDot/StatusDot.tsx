import { statusLabel, statusTone, type ServiceStatus } from './status';

const DOT: Record<string, string> = {
  running: 'bg-running-dot',
  stopped: 'bg-stopped-dot',
  starting: 'bg-warn-dot animate-pulse',
  warn: 'bg-warn-dot',
  danger: 'bg-danger-dot',
};

const TEXT: Record<string, string> = {
  running: 'text-running',
  stopped: 'text-fg-muted',
  starting: 'text-warn',
  warn: 'text-warn',
  danger: 'text-danger',
};

/** Colour is never the only signal: the label is always rendered. */
export function StatusDot({ status }: { status: ServiceStatus }) {
  const tone = statusTone(status);
  return (
    <span className={`inline-flex items-center gap-2 text-sm font-semibold ${TEXT[tone]}`}>
      <span aria-hidden className={`size-2.5 rounded-full ${DOT[tone]}`} />
      {statusLabel(status)}
    </span>
  );
}
