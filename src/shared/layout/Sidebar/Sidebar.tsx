import { setModule, setTheme, type ModuleKey } from '@/app/uiSlice';
import { useAppDispatch, useAppSelector } from '@/app/store';
import { Button } from '@/shared/ui/Button';

const ITEMS: { key: ModuleKey; label: string }[] = [
  { key: 'services', label: 'Services' },
  { key: 'ports', label: 'Ports' },
  { key: 'logs', label: 'Logs' },
  { key: 'settings', label: 'Settings' },
];

export function Sidebar() {
  const dispatch = useAppDispatch();
  const active = useAppSelector((s) => s.ui.activeModule);
  const theme = useAppSelector((s) => s.ui.theme);
  return (
    <nav className="flex w-52 shrink-0 flex-col gap-1 border-r border-border bg-paper p-4">
      <div className="mb-4 font-display text-xl font-extrabold tracking-tight">
        Orbit<span className="text-accent">.</span>
      </div>
      {ITEMS.map(({ key, label }) => (
        <button
          key={key}
          type="button"
          aria-current={active === key ? 'page' : undefined}
          onClick={() => dispatch(setModule(key))}
          className={`rounded-full px-4 py-2 text-left text-sm font-medium transition ${
            active === key ? 'bg-raised text-fg' : 'text-fg-muted hover:bg-raised'
          }`}
        >
          {label}
        </button>
      ))}
      <div className="mt-auto">
        <Button
          size="sm"
          className="w-full"
          onClick={() => dispatch(setTheme(theme === 'dark' ? 'light' : 'dark'))}
        >
          {theme === 'dark' ? 'Light theme' : 'Dark theme'}
        </Button>
      </div>
    </nav>
  );
}
