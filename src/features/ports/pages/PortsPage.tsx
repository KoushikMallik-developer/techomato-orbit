import { useCallback, useMemo, useState } from 'react';
import { useAppDispatch } from '@/app/store';
import { pushToast } from '@/app/uiSlice';
import { CMD } from '@/shared/ipc/commands';
import { ipcVoid } from '@/shared/ipc/invoke';
import { Button } from '@/shared/ui/Button';
import { Network } from 'lucide-react';
import { Card } from '@/shared/ui/Card';
import { EmptyState } from '@/shared/ui/EmptyState';
import { inputClass as input, SearchInput } from '@/shared/ui/Field';
import { PageHeader } from '@/shared/ui/PageHeader';
import { Segmented } from '@/shared/ui/Segmented';
import type { PortRow } from '../api/ports.schema';
import {
  useCheckPort,
  useEndProcess,
  usePorts,
  useWatchAlerts,
  useWatchList,
} from '../api/usePorts';
import { PortTable } from '../components/PortTable';
import { filterPorts, type ProtoFilter } from '../portFilter';

export function PortsPage() {
  const dispatch = useAppDispatch();
  const ports = usePorts();
  const end = useEndProcess();
  const check = useCheckPort();
  const { watch, add, remove } = useWatchList();
  const [query, setQuery] = useState('');
  const [proto, setProto] = useState<ProtoFilter>('all');
  const [onlyMine, setOnlyMine] = useState(false);
  const [checkText, setCheckText] = useState('');
  const [checked, setChecked] = useState<{ port: number; busy: boolean } | null>(null);

  const onConflict = useCallback(
    (r: PortRow) => {
      const who = r.processName ?? 'another program';
      dispatch(pushToast(`Watched port ${r.port} is in use by ${who} (PID ${r.pid}).`));
      // Also raises the native notification and records it in the history.
      ipcVoid(CMD.notifyPortConflict, { port: r.port, process: who, pid: r.pid }).catch(() => {});
    },
    [dispatch],
  );
  useWatchAlerts(ports.data, watch, onConflict);

  const rows = useMemo(
    () => filterPorts(ports.data ?? [], query, proto, onlyMine),
    [ports.data, query, proto, onlyMine],
  );
  const portNumber = Number(checkText);
  const validPort = Number.isInteger(portNumber) && portNumber >= 1 && portNumber <= 65535;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Ports"
        summary={
          ports.data
            ? `${ports.data.filter((r) => r.state === 'LISTEN' || r.proto === 'udp').length} listening · updates every 2 seconds`
            : 'Reading ports'
        }
      />
      <div className="flex flex-wrap items-center gap-3">
        <SearchInput
          aria-label="Search ports"
          placeholder="Search port, process, path or service"
          className="w-80"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Segmented
          label="Protocol"
          value={proto}
          onChange={setProto}
          options={[
            { value: 'all', label: 'All' },
            { value: 'tcp', label: 'TCP' },
            { value: 'udp', label: 'UDP' },
          ]}
        />
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={onlyMine}
            onChange={(e) => setOnlyMine(e.target.checked)}
          />
          Only Orbit services
        </label>
        <form
          className="ml-auto flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (validPort)
              check.mutate(portNumber, {
                onSuccess: (busy) => setChecked({ port: portNumber, busy }),
              });
          }}
        >
          <input
            aria-label="Check a port"
            placeholder="Check a port"
            inputMode="numeric"
            className={`${input} w-32`}
            value={checkText}
            onChange={(e) => {
              setCheckText(e.target.value);
              setChecked(null);
            }}
          />
          <Button size="sm" type="submit" disabled={!validPort || check.isPending}>
            Check
          </Button>
          {checked && (
            <span role="status" className={checked.busy ? 'text-danger' : 'text-running'}>
              {checked.busy ? `Port ${checked.port} is in use` : `Port ${checked.port} is free`}
            </span>
          )}
        </form>
      </div>
      {watch.length > 0 && (
        <Card className="flex flex-wrap items-center gap-2 py-3 text-sm">
          <span className="font-semibold">Watching</span>
          {watch.map((p) => (
            <Button key={p} size="sm" aria-label={`Stop watching ${p}`} onClick={() => remove(p)}>
              {p} ×
            </Button>
          ))}
          <span className="text-fg-muted">
            You are told when something else takes one of these.
          </span>
        </Card>
      )}
      {ports.isError && (
        <Card role="alert" className="text-danger">
          Could not read ports: {ports.error.message}
        </Card>
      )}
      {ports.data && rows.length === 0 && (
        <EmptyState
          icon={Network}
          title="No matching ports"
          hint="Nothing is listening that fits the current filters."
        />
      )}
      {rows.length > 0 && (
        <PortTable
          rows={rows}
          watch={watch}
          busy={end.isPending}
          onEnd={(row) => end.mutate({ row, force: false })}
          onToggleWatch={(p) => (watch.includes(p) ? remove(p) : add(p))}
        />
      )}
    </div>
  );
}
