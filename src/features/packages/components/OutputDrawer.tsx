import { useEffect, useRef } from 'react';
import { CheckCircle2, Loader2, X, XCircle } from 'lucide-react';
import { Button } from '@/shared/ui/Button';
import type { Run } from '../api/usePackages';

/** Live output of the running (or last) package change. */
export function OutputDrawer({ run, onClose }: { run: Run; onClose: () => void }) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView({ block: 'end' });
  }, [run.lines.length]);
  return (
    <section
      aria-label="Package output"
      className="overflow-hidden rounded-card border border-border bg-paper shadow-card"
    >
      <header className="flex items-center gap-2 border-b border-border px-4 py-2.5 text-sm">
        {run.running ? (
          <Loader2 aria-hidden className="size-4 animate-spin text-accent" />
        ) : run.ok ? (
          <CheckCircle2 aria-hidden className="size-4 text-running" />
        ) : (
          <XCircle aria-hidden className="size-4 text-danger" />
        )}
        <span className="font-semibold">{run.label}</span>
        <span className="text-fg-muted">
          {run.running ? 'Running…' : run.ok ? 'Finished' : 'Failed'}
        </span>
        {!run.running && (
          <Button size="sm" className="ml-auto" aria-label="Close output" onClick={onClose}>
            <X aria-hidden className="size-4" />
          </Button>
        )}
      </header>
      <div
        role="log"
        aria-live="off"
        className="max-h-56 overflow-auto bg-canvas px-4 py-2 font-mono text-xs leading-relaxed text-fg-muted"
      >
        {run.lines.length === 0 && <div className="text-fg-subtle">Waiting for output…</div>}
        {run.lines.map((l, i) => (
          <div key={i} className="whitespace-pre-wrap break-all">
            {l}
          </div>
        ))}
        <div ref={end} />
      </div>
    </section>
  );
}
