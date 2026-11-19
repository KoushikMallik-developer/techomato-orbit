import {
  FolderKanban,
  Moon,
  Network,
  Package,
  ScrollText,
  Search,
  ServerCog,
  Settings,
  Sun,
  type LucideIcon,
} from 'lucide-react';
import { setModule, setTheme, type ModuleKey } from '@/app/uiSlice';
import { useAppDispatch, useAppSelector } from '@/app/store';
import { usePrefs } from '@/features/settings/api/usePrefs';
import { useRuntime } from '@/features/services/api/useRuntime';

type Item = {
  key: ModuleKey;
  label: string;
  icon: LucideIcon;
  pref?: 'projects' | 'ports' | 'packages';
};
const RUN: Item[] = [
  { key: 'services', label: 'Services', icon: ServerCog },
  { key: 'projects', label: 'Projects', icon: FolderKanban, pref: 'projects' },
  { key: 'ports', label: 'Ports', icon: Network, pref: 'ports' },
  { key: 'packages', label: 'Packages', icon: Package, pref: 'packages' },
  { key: 'logs', label: 'Logs', icon: ScrollText },
];

export const OPEN_PALETTE_EVENT = 'orbit:open-palette';

function NavButton(props: { item: Item; active: boolean; badge?: number; onClick: () => void }) {
  const { item, active, badge } = props;
  const Icon = item.icon;
  return (
    <button
      type="button"
      aria-current={active ? 'page' : undefined}
      onClick={props.onClick}
      className={`group flex items-center gap-3 rounded-full px-3.5 py-2 text-left text-sm font-medium transition-colors ${
        active ? 'bg-accent-soft text-accent' : 'text-fg-muted hover:bg-raised hover:text-fg'
      }`}
    >
      <Icon aria-hidden className="size-[18px] shrink-0" strokeWidth={active ? 2.25 : 1.75} />
      <span className="flex-1">{item.label}</span>
      {badge ? (
        <span
          aria-label={`${badge} running`}
          className="rounded-full bg-running-soft px-2 text-xs font-semibold text-running"
        >
          {badge}
        </span>
      ) : null}
    </button>
  );
}

export function Sidebar() {
  const dispatch = useAppDispatch();
  const active = useAppSelector((s) => s.ui.activeModule);
  const theme = useAppSelector((s) => s.ui.theme);
  const modules = usePrefs().data?.modules;
  const runtime = useRuntime();
  const running = Object.values(runtime).filter((r) => r.status === 'running').length;
  const visible = (i: Item) => !i.pref || modules?.[i.pref] !== false;
  const go = (key: ModuleKey) => () => dispatch(setModule(key));

  return (
    <nav
      aria-label="Main"
      className="flex w-60 shrink-0 flex-col gap-1 border-r border-border bg-paper px-3 py-4"
    >
      <div className="mb-3 px-3.5 font-display text-xl font-extrabold tracking-tight">
        Orbit<span className="text-accent">.</span>
      </div>
      <button
        type="button"
        onClick={() => window.dispatchEvent(new Event(OPEN_PALETTE_EVENT))}
        className="mb-3 flex items-center gap-2 rounded-full border-[1.5px] border-border bg-canvas px-3.5 py-1.5 text-sm text-fg-subtle transition hover:border-border-strong"
      >
        <Search aria-hidden className="size-4" />
        <span className="flex-1 whitespace-nowrap text-left">Search</span>
        <kbd className="rounded-md border border-border bg-paper px-1.5 font-mono text-[11px]">
          Ctrl K
        </kbd>
      </button>
      {RUN.filter(visible).map((item) => (
        <NavButton
          key={item.key}
          item={item}
          active={active === item.key}
          badge={item.key === 'services' ? running : undefined}
          onClick={go(item.key)}
        />
      ))}
      <div className="mt-auto flex items-center gap-2 border-t border-border pt-3">
        <div className="flex-1">
          <NavButton
            item={{ key: 'settings', label: 'Settings', icon: Settings }}
            active={active === 'settings'}
            onClick={go('settings')}
          />
        </div>
        <button
          type="button"
          aria-label={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
          title={theme === 'dark' ? 'Light theme' : 'Dark theme'}
          onClick={() => dispatch(setTheme(theme === 'dark' ? 'light' : 'dark'))}
          className="grid size-9 place-items-center rounded-full text-fg-muted transition hover:bg-raised hover:text-fg"
        >
          {theme === 'dark' ? (
            <Sun aria-hidden className="size-[18px]" />
          ) : (
            <Moon aria-hidden className="size-[18px]" />
          )}
        </button>
      </div>
    </nav>
  );
}
