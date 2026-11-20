import { useEffect, useRef, useState, type ReactNode } from 'react';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { ipc } from '@/shared/ipc/invoke';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
import { Check, formInputClass } from '@/shared/ui/Field';
import { useProjects } from '@/features/projects/api/useProjects';
import { CATEGORIES, type ServiceSpec } from '../../api/services.schema';
import {
  defaultFormValues,
  formToSpec,
  specToForm,
  type EnvRow,
  type ServiceFormValues,
} from './formToSpec';
import { applyTemplate, SERVICE_TEMPLATES } from './serviceTemplates';

const input = formInputClass;

function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return (
    <label className="flex flex-col gap-1 text-sm font-medium">
      {label}
      {children}
      {hint && <span className="text-xs font-normal text-fg-subtle">{hint}</span>}
    </label>
  );
}

async function pick(folder: boolean): Promise<string | null> {
  return ipc(CMD.pickPath, { folder }, z.string().nullable());
}

type Props = {
  initial: ServiceSpec | null;
  saving: boolean;
  onSave: (spec: ServiceSpec) => void;
  onCancel: () => void;
};

export function ServiceForm({ initial, saving, onSave, onCancel }: Props) {
  const [f, setF] = useState<ServiceFormValues>(() =>
    initial ? specToForm(initial) : defaultFormValues(),
  );
  const projects = useProjects();
  const [error, setError] = useState<string | null>(null);
  const errorBox = useRef<HTMLParagraphElement>(null);
  // The form is long: bring a validation message into view and read it out.
  useEffect(() => {
    if (error) {
      errorBox.current?.scrollIntoView({ block: 'center' });
      errorBox.current?.focus();
    }
  }, [error]);
  const [stored, setStored] = useState<Set<string>>(new Set());
  const set = <K extends keyof ServiceFormValues>(k: K, v: ServiceFormValues[K]) =>
    setF((p) => ({ ...p, [k]: v }));

  // Which secrets already have a stored value, so the form can say "set" without ever seeing it.
  useEffect(() => {
    if (!initial) return;
    const keys = initial.env.filter((e) => e.isSecret).map((e) => e.key);
    void Promise.all(
      keys.map(async (key) => {
        const set = await ipc(CMD.serviceSecretIsSet, { id: initial.id, key }, z.boolean());
        return set ? key : null;
      }),
    ).then((r) => setStored(new Set(r.filter((k): k is string => k !== null))));
  }, [initial]);

  const setEnv = (i: number, patch: Partial<EnvRow>) =>
    set(
      'env',
      f.env.map((r, n) => (n === i ? { ...r, ...patch } : r)),
    );

  function submit(e: React.FormEvent) {
    e.preventDefault();
    try {
      setError(null);
      onSave(formToSpec(f));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <Card>
      <form onSubmit={submit} className="flex flex-col gap-5">
        <h1 className="text-xl font-bold">{initial ? 'Edit service' : 'Add service'}</h1>

        {!initial && (
          <Field label="Start from" hint="Fills the form below. You can change anything.">
            <select
              className={input}
              value=""
              onChange={(e) => {
                const t = SERVICE_TEMPLATES.find((x) => x.id === e.target.value);
                if (t) setF((p) => applyTemplate(p, t));
              }}
            >
              <option value="">Choose a template…</option>
              {SERVICE_TEMPLATES.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                </option>
              ))}
            </select>
          </Field>
        )}

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Name">
            <input
              className={input}
              value={f.name}
              onChange={(e) => set('name', e.target.value)}
              required
            />
          </Field>
          <Field label="Category">
            <select
              className={input}
              value={f.category}
              onChange={(e) => set('category', e.target.value as ServiceFormValues['category'])}
            >
              {CATEGORIES.map((c) => (
                <option key={c}>{c}</option>
              ))}
            </select>
          </Field>
        </div>

        <Field label="Project">
          <select
            className={input}
            value={f.projectId ?? ''}
            onChange={(e) => set('projectId', e.target.value === '' ? null : e.target.value)}
          >
            <option value="">No project</option>
            {(projects.data ?? []).map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </Field>

        <Field label="Executable" hint="A full path, or a name found on PATH.">
          <span className="flex gap-2">
            <input
              aria-label="Executable"
              className={input}
              value={f.executable}
              onChange={(e) => set('executable', e.target.value)}
              required
            />
            <Button
              aria-label="Browse for executable"
              onClick={async () => {
                const p = await pick(false);
                if (p) set('executable', p);
              }}
            >
              Browse
            </Button>
          </span>
        </Field>

        <Field label="Arguments" hint="One per line. Quotes are kept literally.">
          <textarea
            className={`${input} font-mono`}
            rows={3}
            value={f.argsText}
            onChange={(e) => set('argsText', e.target.value)}
          />
        </Field>

        <Field label="Working directory">
          <span className="flex gap-2">
            <input
              aria-label="Working directory"
              className={input}
              value={f.workingDir}
              onChange={(e) => set('workingDir', e.target.value)}
            />
            <Button
              aria-label="Browse for working directory"
              onClick={async () => {
                const p = await pick(true);
                if (p) set('workingDir', p);
              }}
            >
              Browse
            </Button>
          </span>
        </Field>

        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-sm font-medium">Environment variables</legend>
          {f.env.map((r, i) => (
            <div key={i} className="flex flex-wrap items-center gap-2">
              <input
                aria-label={`Key of variable ${i + 1}`}
                placeholder="KEY"
                className={`${input} w-40 font-mono`}
                value={r.key}
                onChange={(e) => setEnv(i, { key: e.target.value })}
              />
              <input
                aria-label={r.key ? `Value of ${r.key}` : `Value of variable ${i + 1}`}
                type={r.isSecret ? 'password' : 'text'}
                placeholder={
                  r.isSecret && stored.has(r.key) ? 'Stored. Leave blank to keep.' : 'value'
                }
                className={`${input} min-w-0 flex-1 font-mono`}
                value={r.value}
                onChange={(e) => setEnv(i, { value: e.target.value })}
              />
              <Check
                label="Secret"
                checked={r.isSecret}
                onChange={(v) => setEnv(i, { isSecret: v })}
              />
              <Button
                size="sm"
                aria-label={r.key ? `Remove ${r.key}` : `Remove variable ${i + 1}`}
                onClick={() =>
                  set(
                    'env',
                    f.env.filter((_, n) => n !== i),
                  )
                }
              >
                Remove
              </Button>
            </div>
          ))}
          <div>
            <Button
              size="sm"
              onClick={() => set('env', [...f.env, { key: '', value: '', isSecret: false }])}
            >
              Add variable
            </Button>
          </div>
        </fieldset>

        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="Port" hint="Checked before start.">
            <input
              className={input}
              inputMode="numeric"
              value={f.port}
              onChange={(e) => set('port', e.target.value)}
            />
          </Field>
          <Field label="Startup timeout (s)">
            <input
              className={input}
              inputMode="numeric"
              value={f.startupTimeoutSecs}
              onChange={(e) => set('startupTimeoutSecs', e.target.value)}
            />
          </Field>
          <Field label="Shutdown timeout (s)">
            <input
              className={input}
              inputMode="numeric"
              value={f.shutdownTimeoutSecs}
              onChange={(e) => set('shutdownTimeoutSecs', e.target.value)}
            />
          </Field>
        </div>

        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="Restart policy">
            <select
              className={input}
              value={f.restartKind}
              onChange={(e) =>
                set('restartKind', e.target.value as ServiceFormValues['restartKind'])
              }
            >
              <option value="never">Never</option>
              <option value="onFailure">On failure</option>
              <option value="always">Always</option>
            </select>
          </Field>
          {f.restartKind !== 'never' && (
            <>
              <Field label="Max retries">
                <input
                  className={input}
                  inputMode="numeric"
                  value={f.maxRetries}
                  onChange={(e) => set('maxRetries', e.target.value)}
                />
              </Field>
              <Field label="Backoff (s)">
                <input
                  className={input}
                  inputMode="numeric"
                  value={f.backoffSecs}
                  onChange={(e) => set('backoffSecs', e.target.value)}
                />
              </Field>
            </>
          )}
        </div>

        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="Health check">
            <select
              className={input}
              value={f.healthKind}
              onChange={(e) => set('healthKind', e.target.value as ServiceFormValues['healthKind'])}
            >
              <option value="none">None</option>
              <option value="tcp">TCP port</option>
              <option value="http">HTTP</option>
              <option value="command">Command</option>
            </select>
          </Field>
          {f.healthKind === 'tcp' && (
            <Field label="Port">
              <input
                className={input}
                inputMode="numeric"
                value={f.healthPort}
                onChange={(e) => set('healthPort', e.target.value)}
              />
            </Field>
          )}
          {f.healthKind === 'http' && (
            <>
              <Field label="URL">
                <input
                  className={input}
                  placeholder="http://localhost:8080/health"
                  value={f.healthUrl}
                  onChange={(e) => set('healthUrl', e.target.value)}
                />
              </Field>
              <Field label="Accepted status">
                <span className="flex items-center gap-2">
                  <input
                    aria-label="Lowest status"
                    className={input}
                    value={f.healthMin}
                    onChange={(e) => set('healthMin', e.target.value)}
                  />
                  –
                  <input
                    aria-label="Highest status"
                    className={input}
                    value={f.healthMax}
                    onChange={(e) => set('healthMax', e.target.value)}
                  />
                </span>
              </Field>
            </>
          )}
          {f.healthKind === 'command' && (
            <>
              <Field label="Executable">
                <input
                  className={input}
                  value={f.healthExe}
                  onChange={(e) => set('healthExe', e.target.value)}
                />
              </Field>
              <Field label="Arguments" hint="One per line.">
                <textarea
                  className={`${input} font-mono`}
                  rows={2}
                  value={f.healthArgsText}
                  onChange={(e) => set('healthArgsText', e.target.value)}
                />
              </Field>
            </>
          )}
          {f.healthKind !== 'none' && (
            <>
              <Field label="Interval (s)">
                <input
                  className={input}
                  inputMode="numeric"
                  value={f.healthInterval}
                  onChange={(e) => set('healthInterval', e.target.value)}
                />
              </Field>
              <Field label="Timeout (s)">
                <input
                  className={input}
                  inputMode="numeric"
                  value={f.healthTimeout}
                  onChange={(e) => set('healthTimeout', e.target.value)}
                />
              </Field>
            </>
          )}
        </div>

        <div className="flex flex-col gap-2">
          <Check
            label="Run through the shell (cmd /c)"
            checked={f.runThroughShell}
            onChange={(v) => set('runThroughShell', v)}
          />
          <Check
            label="Keep running when Orbit quits"
            checked={f.keepRunningOnQuit}
            onChange={(v) => set('keepRunningOnQuit', v)}
          />
          <Check label="Enabled" checked={f.enabled} onChange={(v) => set('enabled', v)} />
        </div>

        {error && (
          <p ref={errorBox} tabIndex={-1} role="alert" className="text-sm text-danger">
            {error}
          </p>
        )}
        <div className="flex gap-2">
          <Button type="submit" variant="primary" disabled={saving}>
            {saving ? 'Saving…' : 'Save'}
          </Button>
          <Button onClick={onCancel}>Cancel</Button>
        </div>
      </form>
    </Card>
  );
}
