import { useState, type ReactNode } from 'react';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { ipc } from '@/shared/ipc/invoke';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
import { formInputClass, miniSelectClass } from '@/shared/ui/Field';
import type { ServiceSpec } from '@/features/services/api/services.schema';
import { ON_FAILURE, type Edge, type OnFailure, type Project } from '../api/projects.schema';
import { layoutLevels } from '../dependencyGraph';

const input = formInputClass;

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1 text-sm font-medium">
      {label}
      {children}
    </label>
  );
}

type Props = {
  /** `null` while creating. */
  initial: Project | null;
  services: ServiceSpec[];
  edges: Edge[];
  saving: boolean;
  onCancel: () => void;
  /** Saves the project fields and which services belong to it. */
  onSave: (project: Project, memberIds: string[]) => void;
  onSetDeps: (serviceId: string, deps: Pick<Edge, 'dependsOnId' | 'onFailure'>[]) => void;
};

export function ProjectEditor({
  initial,
  services,
  edges,
  saving,
  onCancel,
  onSave,
  onSetDeps,
}: Props) {
  const [name, setName] = useState(initial?.name ?? '');
  const [description, setDescription] = useState(initial?.description ?? '');
  const [icon, setIcon] = useState(initial?.icon ?? '');
  const [rootDir, setRootDir] = useState(initial?.rootDir ?? '');
  const [members, setMembers] = useState<Set<string>>(
    new Set(services.filter((s) => initial && s.projectId === initial.id).map((s) => s.id)),
  );
  const toggle = (id: string) =>
    setMembers((old) => {
      const next = new Set(old);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const memberSpecs = services.filter((s) => members.has(s.id));
  const memberEdges = edges.filter((e) => members.has(e.serviceId));
  const levels = layoutLevels(memberSpecs, memberEdges);
  const nameOf = (id: string) => services.find((s) => s.id === id)?.name ?? 'Unknown service';

  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        onSave(
          {
            id: initial?.id ?? '',
            name: name.trim(),
            description,
            icon,
            rootDir,
            archived: initial?.archived ?? false,
          },
          [...members],
        );
      }}
    >
      <h1 className="text-3xl font-extrabold">{initial ? 'Edit project' : 'New project'}</h1>
      <Card className="flex flex-col gap-4">
        <div className="grid gap-4 sm:grid-cols-[1fr_8rem]">
          <Field label="Name">
            <input
              className={input}
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
            />
          </Field>
          <Field label="Icon">
            <input
              className={input}
              value={icon}
              maxLength={4}
              placeholder="🍅"
              onChange={(e) => setIcon(e.target.value)}
            />
          </Field>
        </div>
        <Field label="Description">
          <input
            className={input}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </Field>
        <Field label="Root directory">
          <span className="flex gap-2">
            <input
              aria-label="Root directory"
              className={input}
              value={rootDir}
              onChange={(e) => setRootDir(e.target.value)}
            />
            <Button
              aria-label="Browse for root directory"
              onClick={async () => {
                const p = await ipc(CMD.pickPath, { folder: true }, z.string().nullable());
                if (p) setRootDir(p);
              }}
            >
              Browse
            </Button>
          </span>
        </Field>
      </Card>

      <Card role="group" aria-label="Services in this project" className="flex flex-col gap-2">
        <h2 className="text-lg font-bold">Services in this project</h2>
        {services.length === 0 && (
          <p className="text-sm text-fg-muted">Add services on the Services page first.</p>
        )}
        {services.map((s) => (
          <label key={s.id} className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={members.has(s.id)} onChange={() => toggle(s.id)} />
            {s.name}
            {s.projectId && s.projectId !== initial?.id && !members.has(s.id) && (
              <span className="text-xs text-fg-subtle">(in another project)</span>
            )}
            {s.projectId && s.projectId !== initial?.id && members.has(s.id) && (
              <span className="text-xs text-fg-subtle">(will move here)</span>
            )}
          </label>
        ))}
      </Card>

      {initial ? (
        <Card className="flex flex-col gap-3">
          <h2 className="text-lg font-bold">Start order</h2>
          <p className="text-sm text-fg-muted">
            A service starts after the services it needs are running. If a needed service fails,
            choose whether to stop there or carry on. Changes here are saved as soon as you make
            them; Cancel does not undo them.
          </p>
          {memberSpecs.map((s) => {
            const mine = edges.filter((e) => e.serviceId === s.id);
            const candidates = services.filter(
              (c) => c.id !== s.id && !mine.some((e) => e.dependsOnId === c.id),
            );
            const save = (deps: Edge[]) =>
              onSetDeps(
                s.id,
                deps.map(({ dependsOnId, onFailure }) => ({ dependsOnId, onFailure })),
              );
            return (
              <div key={s.id} className="flex flex-col gap-1 border-t border-border pt-2 text-sm">
                <span className="font-semibold">{s.name} needs</span>
                {mine.length === 0 && <span className="text-fg-muted">Nothing</span>}
                {mine.map((e) => (
                  <span key={e.dependsOnId} className="flex flex-wrap items-center gap-2">
                    {nameOf(e.dependsOnId)}
                    <select
                      aria-label={`If ${nameOf(e.dependsOnId)} fails`}
                      className={miniSelectClass}
                      value={e.onFailure}
                      onChange={(ev) =>
                        save(
                          mine.map((m) =>
                            m === e ? { ...m, onFailure: ev.target.value as OnFailure } : m,
                          ),
                        )
                      }
                    >
                      {ON_FAILURE.map((o) => (
                        <option key={o} value={o}>
                          {o === 'abort' ? 'If it fails: stop here' : 'If it fails: carry on'}
                        </option>
                      ))}
                    </select>
                    <Button
                      size="sm"
                      aria-label={`Remove ${nameOf(e.dependsOnId)} from ${s.name}`}
                      onClick={() => save(mine.filter((m) => m !== e))}
                    >
                      Remove
                    </Button>
                  </span>
                ))}
                {candidates.length > 0 && (
                  <select
                    aria-label={`Add a service ${s.name} needs`}
                    className={`w-fit ${miniSelectClass}`}
                    value=""
                    onChange={(ev) => {
                      if (ev.target.value)
                        save([
                          ...mine,
                          { serviceId: s.id, dependsOnId: ev.target.value, onFailure: 'abort' },
                        ]);
                    }}
                  >
                    <option value="">Add a service it needs…</option>
                    {candidates.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.name}
                      </option>
                    ))}
                  </select>
                )}
              </div>
            );
          })}
          <div className="border-t border-border pt-2 text-sm">
            <span className="font-semibold">Order</span>
            <ol aria-label="Start order" className="mt-1 list-decimal pl-5">
              {levels.map((level, i) => (
                <li key={i}>
                  {level.map(nameOf).join(', ')}
                  {level.length > 1 && <span className="text-fg-subtle"> (together)</span>}
                </li>
              ))}
            </ol>
          </div>
        </Card>
      ) : (
        <p className="text-sm text-fg-muted">
          Save the project first, then Edit it to set which services must start before others.
        </p>
      )}

      <div className="flex gap-2">
        <Button type="submit" variant="primary" disabled={saving || name.trim() === ''}>
          {saving ? 'Saving…' : 'Save'}
        </Button>
        <Button onClick={onCancel}>Cancel</Button>
        {name.trim() === '' && (
          <span className="self-center text-sm text-fg-muted">Enter a name to save.</span>
        )}
      </div>
    </form>
  );
}
