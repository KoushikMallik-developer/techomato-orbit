import { Card } from '@/shared/ui/Card';
import { useAutostart, usePrefs, useSetPrefs, useShortcuts, type Prefs } from '../api/usePrefs';

const MODULES: { key: keyof Prefs['modules']; label: string }[] = [
  { key: 'projects', label: 'Projects' },
  { key: 'ports', label: 'Ports' },
  { key: 'winservices', label: 'Windows services' },
];

const EVENTS: { key: keyof Prefs['notify']; label: string }[] = [
  { key: 'crashed', label: 'A service crashes' },
  { key: 'startFailed', label: 'A service fails to start' },
  { key: 'healthFailed', label: 'A health check fails' },
  { key: 'portConflict', label: 'A watched port is taken' },
  { key: 'autoRestart', label: 'A service is restarted automatically' },
  { key: 'started', label: 'A service starts' },
  { key: 'stopped', label: 'A service stops' },
];

function Check(props: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex items-center gap-3 py-1 text-sm">
      <input
        type="checkbox"
        checked={props.checked}
        onChange={(e) => props.onChange(e.target.checked)}
      />
      {props.label}
    </label>
  );
}

export function SettingsPage() {
  const prefs = usePrefs();
  const save = useSetPrefs();
  const shortcuts = useShortcuts();
  const autostart = useAutostart();
  const p = prefs.data;
  if (!p) return <h1 className="text-3xl font-extrabold">Settings</h1>;

  return (
    <div className="flex max-w-2xl flex-col gap-4">
      <h1 className="text-3xl font-extrabold">Settings</h1>
      <Card>
        <h2 className="mb-2 text-lg font-bold">Startup</h2>
        <Check
          label="Start Orbit with Windows (in the tray)"
          checked={autostart.enabled}
          onChange={(v) => {
            autostart.set(v);
            save.mutate({ ...p, startWithWindows: v });
          }}
        />
        <Check
          label="Keep refreshing while the window is hidden"
          checked={p.refreshHidden}
          onChange={(v) => save.mutate({ ...p, refreshHidden: v })}
        />
      </Card>
      <Card>
        <h2 className="mb-2 text-lg font-bold">Modules</h2>
        <p className="mb-2 text-sm text-fg-muted">Turned-off modules disappear from the menu.</p>
        {MODULES.map((m) => (
          <Check
            key={m.key}
            label={m.label}
            checked={p.modules[m.key]}
            onChange={(v) => save.mutate({ ...p, modules: { ...p.modules, [m.key]: v } })}
          />
        ))}
      </Card>
      <Card>
        <h2 className="mb-2 text-lg font-bold">Notify me when</h2>
        {EVENTS.map((e) => (
          <Check
            key={e.key}
            label={e.label}
            checked={p.notify[e.key]}
            onChange={(v) => save.mutate({ ...p, notify: { ...p.notify, [e.key]: v } })}
          />
        ))}
      </Card>
      <Card>
        <h2 className="mb-2 text-lg font-bold">Shortcuts</h2>
        <p className="mb-2 text-sm text-fg-muted">
          Ctrl+K opens the command palette inside Orbit. These work from anywhere in Windows.
        </p>
        <ul className="flex flex-col gap-1 text-sm">
          {shortcuts.data?.map((s) => (
            <li key={s.accelerator} className="flex flex-wrap items-center gap-3">
              <kbd className="rounded border border-border bg-raised px-2 py-0.5 font-mono text-xs">
                {s.accelerator.replace('Super', 'Win')}
              </kbd>
              <span>{s.label}</span>
              {s.error && (
                <span role="alert" className="text-danger">
                  Not active: {s.error}
                </span>
              )}
            </li>
          ))}
        </ul>
      </Card>
    </div>
  );
}
