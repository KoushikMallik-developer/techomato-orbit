import { useMemo, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { z } from 'zod';
import { useAppDispatch, useAppSelector } from '@/app/store';
import { showLogs } from '@/app/uiSlice';
import { useServices } from '@/features/services';
import { CMD } from '@/shared/ipc/commands';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
import { useLogBuffer } from '../api/useLogs';
import { LogViewer } from '../components/LogViewer';
import { filterLines, type Stream } from '../logFilter';

const ALL: ReadonlySet<Stream> = new Set<Stream>(['stdout', 'stderr']);

export function LogsPage() {
  const dispatch = useAppDispatch();
  const serviceId = useAppSelector((s) => s.ui.logServiceId);
  const services = useServices();
  const { lines, clear } = useLogBuffer(serviceId);
  const [query, setQuery] = useState('');
  const [streams, setStreams] = useState<ReadonlySet<Stream>>(ALL);
  const [paused, setPaused] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [frozen, setFrozen] = useState<typeof lines>([]);

  const shown = useMemo(
    () => filterLines(paused ? frozen : lines, query, streams),
    [paused, frozen, lines, query, streams],
  );

  const clearLogs = useMutation({
    mutationFn: () => ipcVoid(CMD.logClear, { serviceId }),
    onSuccess: clear,
  });
  const exportLogs = useMutation({
    mutationFn: () => ipc(CMD.logExport, { serviceId }, z.boolean()),
  });

  const toggleStream = (s: Stream) =>
    setStreams((cur) => {
      const next = new Set(cur);
      if (next.has(s)) next.delete(s);
      else next.add(s);
      return next;
    });

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-3xl font-extrabold">Logs</h1>
        <select
          aria-label="Service"
          className="ml-auto rounded-full border border-border bg-paper px-4 py-2 text-sm"
          value={serviceId ?? ''}
          onChange={(e) => e.target.value && dispatch(showLogs(e.target.value))}
        >
          <option value="">Choose a service…</option>
          {services.data?.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
      </div>

      {!serviceId ? (
        <Card>Choose a service to see its output.</Card>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <input
              aria-label="Search logs"
              placeholder="Search…"
              className="min-w-48 rounded-full border border-border bg-paper px-4 py-2 text-sm"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <label className="flex items-center gap-1 text-sm">
              <input
                type="checkbox"
                checked={streams.has('stdout')}
                onChange={() => toggleStream('stdout')}
              />
              stdout
            </label>
            <label className="flex items-center gap-1 text-sm">
              <input
                type="checkbox"
                checked={streams.has('stderr')}
                onChange={() => toggleStream('stderr')}
              />
              stderr
            </label>
            <label className="flex items-center gap-1 text-sm">
              <input
                type="checkbox"
                checked={autoScroll}
                onChange={(e) => setAutoScroll(e.target.checked)}
              />
              Auto-scroll
            </label>
            <span className="ml-auto flex flex-wrap gap-2">
              <Button
                size="sm"
                onClick={() => {
                  if (!paused) setFrozen(lines);
                  setPaused(!paused);
                }}
              >
                {paused ? 'Resume' : 'Pause'}
              </Button>
              <Button
                size="sm"
                onClick={() =>
                  void navigator.clipboard.writeText(shown.map((l) => l.text).join('\n'))
                }
              >
                Copy
              </Button>
              <Button size="sm" disabled={exportLogs.isPending} onClick={() => exportLogs.mutate()}>
                Export
              </Button>
              <Button
                size="sm"
                variant="danger"
                disabled={clearLogs.isPending}
                onClick={() => clearLogs.mutate()}
              >
                Clear
              </Button>
            </span>
          </div>
          <div className="min-h-0 flex-1">
            {shown.length === 0 ? (
              <Card className="text-sm text-fg-muted">
                {lines.length === 0 ? 'No output yet.' : 'No lines match the current filter.'}
              </Card>
            ) : (
              <LogViewer lines={shown} follow={autoScroll && !paused} />
            )}
          </div>
        </>
      )}
    </div>
  );
}
