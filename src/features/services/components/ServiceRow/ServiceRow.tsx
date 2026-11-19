import { useEffect, useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
import { Pill } from '@/shared/ui/Pill';
import { StatusDot } from '@/shared/ui/StatusDot';
import type { ServiceSpec } from '../../api/services.schema';
import type { Runtime } from '../../api/useRuntime';
import { ServiceActions } from '../ServiceActions/ServiceActions';

export function formatUptime(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s % 60}s`;
  return `${s}s`;
}

function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [active]);
  return now;
}

type Props = {
  spec: ServiceSpec;
  runtime: Runtime | undefined;
  onEdit: () => void;
  onDelete: () => void;
  onLogs: () => void;
};

export function ServiceRow({ spec, runtime, onEdit, onDelete, onLogs }: Props) {
  const status = runtime?.status ?? 'stopped';
  const live = runtime?.pid != null && runtime.startedAtMs != null;
  const now = useNow(live);
  return (
    <Card className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <h2 className="min-w-0 truncate text-lg font-bold">{spec.name}</h2>
        <Pill>{spec.category}</Pill>
        <StatusDot status={status} />
        <dl className="ml-auto flex gap-5 text-sm text-fg-muted">
          <div>
            <dt className="text-xs text-fg-subtle">PID</dt>
            <dd className="font-mono">{runtime?.pid ?? '–'}</dd>
          </div>
          <div>
            <dt className="text-xs text-fg-subtle">Uptime</dt>
            <dd className="font-mono">
              {live ? formatUptime(now - (runtime.startedAtMs ?? now)) : '–'}
            </dd>
          </div>
          <div>
            <dt className="text-xs text-fg-subtle">Port</dt>
            <dd className="font-mono">{spec.port ?? '–'}</dd>
          </div>
        </dl>
      </div>
      {runtime?.lastError && (
        <p className="text-sm text-danger" role="status">
          {runtime.lastError}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <ServiceActions id={spec.id} status={status} />
        <span className="ml-auto flex gap-2">
          <Button size="sm" onClick={onLogs}>
            Logs
          </Button>
          <Button size="sm" onClick={onEdit}>
            Edit
          </Button>
          <Button size="sm" variant="danger" onClick={onDelete}>
            Delete
          </Button>
        </span>
      </div>
    </Card>
  );
}
