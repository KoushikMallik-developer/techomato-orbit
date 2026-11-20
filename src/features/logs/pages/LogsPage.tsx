import { useMemo, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { z } from 'zod';
import { useAppDispatch, useAppSelector } from '@/app/store';
import { pushToast, showLogs, setModule } from '@/app/uiSlice';
import { useServices } from '@/features/services';
import { CMD } from '@/shared/ipc/commands';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
import { PageHeader } from '@/shared/ui/PageHeader';
import { Check, SearchInput, Select } from '@/shared/ui/Field';
import { useLogBuffer } from '../api/useLogs';
import { LogViewer } from '../components/LogViewer';
import { filterLines, type Stream } from '../logFilter';

const ALL: ReadonlySet<Stream> = new Set<Stream>(['stdout', 'stderr']);

export function LogsPage() {
  const dispatch = useAppDispatch();
  const picked = useAppSelector((s) => s.ui.logServiceId);
  const services = useServices();
  // A deleted service is no longer a choice.
  const serviceId = services.data?.some((s) => s.id === picked) ? picked : null;
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
    onSuccess: (saved) => saved && dispatch(pushToast('Log saved.', 'info')),
  });
  const copy = (text: string) =>
    navigator.clipboard.writeText(text).then(
      () => dispatch(pushToast('Copied to the clipboard.', 'info')),
      () => dispatch(pushToast('Could not copy to the clipboard.')),
    );

  const toggleStream = (s: Stream) =>
    setStreams((cur) => {
      const next = new Set(cur);
      if (next.has(s)) next.delete(s);
      else next.add(s);
      return next;
    });

  return (
    <div className="flex h-full flex-col gap-4">
      <PageHeader
        title="Logs"
        summary={serviceId ? 'Live output of the selected service' : 'Pick a service to follow'}
        actions={
          <Select
            aria-label="Service"
            value={serviceId ?? ''}
            onChange={(e) => e.target.value && dispatch(showLogs(e.target.value))}
          >
            <option value="">Choose a service…</option>
            {services.data?.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </Select>
        }
      />

      {!serviceId ? (
        <Card className="flex flex-wrap items-center gap-3">
          {services.data?.length === 0 ? (
            <>
              <span>There are no services yet, so there is no output to show.</span>
              <Button size="sm" onClick={() => dispatch(setModule('services'))}>
                Go to Services
              </Button>
            </>
          ) : (
            'Choose a service to see its output.'
          )}
        </Card>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <SearchInput
              aria-label="Search logs"
              placeholder="Search…"
              className="min-w-56"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <div role="group" aria-label="Show" className="flex items-center gap-3">
              <Check
                label="stdout"
                checked={streams.has('stdout')}
                onChange={() => toggleStream('stdout')}
              />
              <Check
                label="stderr"
                checked={streams.has('stderr')}
                onChange={() => toggleStream('stderr')}
              />
              <Check label="Auto-scroll" checked={autoScroll} onChange={setAutoScroll} />
            </div>
            <span className="ml-auto flex flex-wrap gap-2">
              <Button
                size="sm"
                aria-pressed={paused}
                onClick={() => {
                  if (!paused) setFrozen(lines);
                  setPaused(!paused);
                }}
              >
                {paused ? 'Resume' : 'Pause'}
              </Button>
              <Button size="sm" onClick={() => void copy(shown.map((l) => l.text).join('\n'))}>
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
