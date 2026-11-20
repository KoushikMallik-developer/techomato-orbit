import { useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
import { Pill } from '@/shared/ui/Pill';
import { StatusDot } from '@/shared/ui/StatusDot';
import type { ServiceSpec } from '@/features/services/api/services.schema';
import type { Runtime } from '@/features/services/api/useRuntime';
import type { Project } from '../api/projects.schema';
import { projectStatus, type ProjectStatus } from '../dependencyGraph';

const LABEL: Record<ProjectStatus, string> = {
  running: 'Running',
  partial: 'Partly running',
  failed: 'Has a failed service',
  stopped: 'Stopped',
};

const STEP: Record<string, string> = {
  starting: 'Starting…',
  ready: 'Ready',
  failed: 'Failed',
  skipped: 'Skipped',
  alreadyRunning: 'Already running',
  stopping: 'Stopping…',
  stopped: 'Stopped',
};

type Props = {
  project: Project;
  members: ServiceSpec[];
  runtimes: Record<string, Runtime>;
  progress: Record<string, string>;
  busy: boolean;
  onStart: (selection: string[] | null) => void;
  onStop: () => void;
  onRestart: () => void;
  onStopSelected: (ids: string[]) => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onArchive: () => void;
  onDelete: () => void;
};

export function ProjectCard(p: Props) {
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const statuses = p.members.map((m) => p.runtimes[m.id]?.status ?? 'stopped');
  const roll = projectStatus(statuses);
  const toggle = (id: string) =>
    setPicked((old) => {
      const next = new Set(old);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const chosen = p.members.filter((m) => picked.has(m.id)).map((m) => m.id);
  const empty = p.members.length === 0;
  return (
    <Card className={`flex flex-col gap-3 ${p.project.archived ? 'opacity-70' : ''}`}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        {p.project.icon && <span aria-hidden>{p.project.icon}</span>}
        <h2 className="min-w-0 truncate text-lg font-bold">{p.project.name}</h2>
        {p.project.archived && <Pill>Archived</Pill>}
        <Pill tone={roll === 'running' ? 'running' : 'neutral'}>{LABEL[roll]}</Pill>
        <span className="text-sm text-fg-muted">
          {p.members.length} {p.members.length === 1 ? 'service' : 'services'}
        </span>
      </div>
      {p.project.description && <p className="text-sm text-fg-muted">{p.project.description}</p>}
      {empty ? (
        <p className="text-sm text-fg-muted">
          No services yet. Use Edit to add the services that belong to this project.
        </p>
      ) : (
        <ul className="flex flex-col gap-1">
          {p.members.map((m) => (
            <li key={m.id} className="flex items-center gap-3 text-sm">
              <input
                type="checkbox"
                aria-label={`Select ${m.name}`}
                checked={picked.has(m.id)}
                onChange={() => toggle(m.id)}
              />
              <span className="font-medium">{m.name}</span>
              <StatusDot status={p.runtimes[m.id]?.status ?? 'stopped'} />
              {p.progress[m.id] && (
                <span className="text-xs text-fg-subtle">{STEP[p.progress[m.id]] ?? ''}</span>
              )}
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="primary"
          disabled={empty || p.busy}
          onClick={() => p.onStart(null)}
        >
          Start all
        </Button>
        <Button size="sm" disabled={empty || p.busy} onClick={p.onStop}>
          Stop all
        </Button>
        <Button size="sm" disabled={empty || p.busy} onClick={p.onRestart}>
          Restart all
        </Button>
        <Button
          size="sm"
          disabled={chosen.length === 0 || p.busy}
          onClick={() => p.onStart(chosen)}
        >
          Start selected
        </Button>
        <Button
          size="sm"
          disabled={chosen.length === 0 || p.busy}
          onClick={() => p.onStopSelected(chosen)}
        >
          Stop selected
        </Button>
        <span className="ml-auto flex gap-2">
          <Button size="sm" aria-label={`Edit ${p.project.name}`} onClick={p.onEdit}>
            Edit
          </Button>
          <Button size="sm" aria-label={`Duplicate ${p.project.name}`} onClick={p.onDuplicate}>
            Duplicate
          </Button>
          <Button
            size="sm"
            aria-label={`${p.project.archived ? 'Unarchive' : 'Archive'} ${p.project.name}`}
            onClick={p.onArchive}
          >
            {p.project.archived ? 'Unarchive' : 'Archive'}
          </Button>
          <Button
            size="sm"
            variant="danger"
            aria-label={`Delete ${p.project.name}`}
            onClick={p.onDelete}
          >
            Delete
          </Button>
        </span>
      </div>
    </Card>
  );
}
