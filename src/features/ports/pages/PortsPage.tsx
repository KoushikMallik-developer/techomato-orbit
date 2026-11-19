import { useCallback, useMemo, useState } from 'react';
import { useAppDispatch } from '@/app/store';
import { pushToast } from '@/app/uiSlice';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
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

const input =
  'rounded-full border-[1.5px] border-border bg-paper px-4 py-1.5 text-sm outline-none focus:border-fg';

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
    (r: PortRow) =>
      dispatch(
        pushToast(
          `Watched port ${r.port} is in use by ${r.processName ?? 'another program'} (PID ${r.pid}).`,
        ),
      ),
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
      <h1 className="text-3xl font-extrabold">Ports</h1>
      <div className="flex flex-wrap items-center gap-3">
        <input
          aria-label="Search ports"
          placeholder="Search port, process, path or service"
          className={`${input} w-72`}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <select
          aria-label="Protocol"
          className={input}
          value={proto}
          onChange={(e) => setProto(e.target.value as ProtoFilter)}
        >
          <option value="all">TCP and UDP</option>
          <option value="tcp">TCP</option>
          <option value="udp">UDP</option>
        </select>
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
        <Card>
          <p className="font-medium">No matching ports.</p>
          <p className="text-sm text-fg-muted">
            Nothing is listening that fits the current filters.
          </p>
        </Card>
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
