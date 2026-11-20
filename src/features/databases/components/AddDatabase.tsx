import { useState } from 'react';
import { IpcError } from '@/shared/ipc/invoke';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
import { formInputClass } from '@/shared/ui/Field';
import { megabytes, useCreateDatabase, useEngines, type Engine } from '../api/useDatabases';

const input = formInputClass;

const PHASE: Record<string, string> = {
  download: 'Downloading',
  unpack: 'Unpacking',
  init: 'Preparing the data folder',
};

/** Things worth knowing before picking an engine. */
const NOTE: Record<string, string> = {
  redis: 'Community Windows port of Redis 5 (there is no official Windows build).',
};

type Props = { onDone: () => void; onCancel: () => void };

/** Pick an engine, name it, and Orbit downloads it once and adds a ready-to-start service. */
export function AddDatabase({ onDone, onCancel }: Props) {
  const engines = useEngines();
  const { create, progress, cancel, busy } = useCreateDatabase();
  const [picked, setPicked] = useState<Engine | null>(null);
  const [name, setName] = useState('');

  function pick(e: Engine) {
    setPicked(e);
    if (name === '' || engines.data?.some((x) => x.name === name)) setName(e.name);
  }

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!picked) return;
    create.mutate({ engine: picked.id, name: name.trim() || picked.name }, { onSuccess: onDone });
  }

  const pct =
    progress && progress.total > 0 ? Math.min(100, (progress.done / progress.total) * 100) : null;

  return (
    <Card>
      <form onSubmit={submit} className="flex flex-col gap-5">
        <h1 className="text-xl font-bold">Add database</h1>
        <p className="text-sm text-fg-muted">
          Orbit downloads the program once, checks it against a fixed checksum, and adds a service
          that runs it on this PC only. Nothing needs Docker.
        </p>
        <p className="text-sm text-fg-muted">
          These databases have no password, so anyone who can sign in to this PC can connect, and
          through the database can run programs as you. Use them for development only.
        </p>

        {engines.isError && (
          <p role="alert" className="text-sm text-danger">
            Could not load the list: {engines.error.message}
          </p>
        )}
        <div
          className="grid gap-3 sm:grid-cols-2"
          role="radiogroup"
          aria-label="Database"
          onKeyDown={(ev) => {
            const list = engines.data ?? [];
            const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[ev.key];
            if (!step || busy || list.length === 0) return;
            ev.preventDefault();
            const at = list.findIndex((x) => x.id === picked?.id);
            pick(list[(at + step + list.length) % list.length]);
            // Move focus with the selection, as a native radio group does.
            requestAnimationFrame(() =>
              ev.currentTarget.querySelector<HTMLElement>('[aria-checked="true"]')?.focus(),
            );
          }}
        >
          {engines.data?.map((e, i) => (
            <button
              key={e.id}
              type="button"
              role="radio"
              aria-checked={picked?.id === e.id}
              // One tab stop for the whole group; arrow keys move inside it.
              tabIndex={picked ? (picked.id === e.id ? 0 : -1) : i === 0 ? 0 : -1}
              disabled={busy}
              onClick={() => pick(e)}
              className={`rounded-card border p-4 text-left transition-colors ${
                picked?.id === e.id ? 'border-accent bg-accent/10' : 'border-border hover:bg-raised'
              }`}
            >
              <span className="block font-semibold">{e.name}</span>
              <span className="block text-xs text-fg-subtle">
                Version {e.version} · port {e.defaultPort} ·{' '}
                {e.installed ? 'already downloaded' : `${megabytes(e.bytes)} download`}
              </span>
              {NOTE[e.id] && <span className="mt-1 block text-xs text-fg-muted">{NOTE[e.id]}</span>}
            </button>
          ))}
        </div>

        {picked && (
          <label className="flex flex-col gap-1 text-sm font-medium">
            Name
            <input
              className={input}
              value={name}
              maxLength={60}
              disabled={busy}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
        )}

        {busy && (
          <div role="status" className="flex flex-col gap-1 text-sm">
            <span>
              {progress ? (PHASE[progress.phase] ?? progress.phase) : 'Starting'}
              {pct !== null && ` · ${Math.round(pct)}%`}…
            </span>
            <div className="h-2 overflow-hidden rounded-full bg-raised">
              <div
                className={`h-full bg-accent transition-[width] ${pct === null ? 'w-1/3 motion-safe:animate-pulse' : ''}`}
                style={pct === null ? undefined : { width: `${pct}%` }}
              />
            </div>
          </div>
        )}
        {create.isError &&
          !(create.error instanceof IpcError && create.error.kind === 'cancelled') && (
            <p role="alert" className="text-sm text-danger">
              {create.error.message}
            </p>
          )}

        <div className="flex gap-2">
          <Button type="submit" variant="primary" disabled={!picked || busy}>
            {busy ? 'Setting up…' : 'Create'}
          </Button>
          <Button
            onClick={busy ? cancel : onCancel}
            disabled={busy && progress?.phase !== 'download'}
          >
            {busy ? 'Cancel download' : 'Cancel'}
          </Button>
        </div>
      </form>
    </Card>
  );
}
