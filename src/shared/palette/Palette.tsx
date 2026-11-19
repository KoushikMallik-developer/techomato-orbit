import { useEffect, useMemo, useRef, useState } from 'react';
import { z } from 'zod';
import { useAppDispatch, useAppSelector } from '@/app/store';
import { pushToast, setModule, setTheme, type ModuleKey } from '@/app/uiSlice';
import { usePrefs } from '@/features/settings/api/usePrefs';
import { useProjects, useRunProject } from '@/features/projects/api/useProjects';
import { useServiceActions } from '@/features/services/api/useRuntime';
import { useServices } from '@/features/services/api/useServices';
import { CMD } from '@/shared/ipc/commands';
import { ipc } from '@/shared/ipc/invoke';
import { rankCommands, type Command } from './rankCommands';

type Item = Command & { run: () => void };

/** Ctrl+K command palette: go to a page, start/stop/restart things, check a port, change theme. */
export function Palette() {
  const dispatch = useAppDispatch();
  const theme = useAppSelector((s) => s.ui.theme);
  const prefs = usePrefs().data;
  const services = useServices().data ?? [];
  const projects = useProjects().data ?? [];
  const svc = useServiceActions();
  const proj = useRunProject();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [cursor, setCursor] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setOpen((o) => !o);
        setQuery('');
        setCursor(0);
      } else if (e.key === 'Escape') setOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
  useEffect(() => {
    if (open) input.current?.focus();
  }, [open]);

  const items = useMemo(() => {
    const go = (key: ModuleKey, title: string): Item => ({
      id: `go-${key}`,
      title,
      keywords: ['open', 'navigate'],
      run: () => dispatch(setModule(key)),
    });
    const list: Item[] = [go('services', 'Go to Services'), go('logs', 'Go to Logs')];
    if (prefs?.modules.projects !== false) list.push(go('projects', 'Go to Projects'));
    if (prefs?.modules.ports !== false) list.push(go('ports', 'Go to Ports'));
    list.push(go('settings', 'Open settings'), {
      id: 'theme',
      title: 'Toggle theme',
      keywords: ['dark', 'light'],
      run: () => dispatch(setTheme(theme === 'dark' ? 'light' : 'dark')),
    });
    for (const s of services) {
      list.push(
        { id: `s-start-${s.id}`, title: `Start ${s.name}`, run: () => svc.start.mutate(s.id) },
        { id: `s-stop-${s.id}`, title: `Stop ${s.name}`, run: () => svc.stop.mutate(s.id) },
        {
          id: `s-restart-${s.id}`,
          title: `Restart ${s.name}`,
          run: () => svc.restart.mutate(s.id),
        },
      );
    }
    for (const p of projects.filter((x) => !x.archived)) {
      list.push(
        {
          id: `p-start-${p.id}`,
          title: `Start project ${p.name}`,
          run: () => proj.start.mutate({ id: p.id }),
        },
        {
          id: `p-stop-${p.id}`,
          title: `Stop project ${p.name}`,
          run: () => proj.stop.mutate({ id: p.id }),
        },
        {
          id: `p-restart-${p.id}`,
          title: `Restart project ${p.name}`,
          run: () => proj.restart.mutate({ id: p.id }),
        },
      );
    }
    return list;
  }, [dispatch, theme, prefs, services, projects, svc, proj]);

  const n = Number(query.trim());
  const portItem: Item[] =
    Number.isInteger(n) && n >= 1 && n <= 65535
      ? [
          {
            id: 'check-port',
            title: `Check port ${n}`,
            run: () =>
              void ipc(CMD.portCheck, { port: n }, z.boolean()).then((busy) =>
                dispatch(pushToast(busy ? `Port ${n} is in use.` : `Port ${n} is free.`)),
              ),
          },
        ]
      : [];
  const shown = [...portItem, ...rankCommands(items, query)].slice(0, 12);

  if (!open) return null;
  const choose = (item: Item | undefined) => {
    if (!item) return;
    setOpen(false);
    item.run();
  };
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-24"
      onMouseDown={(e) => e.target === e.currentTarget && setOpen(false)}
    >
      <div
        role="dialog"
        aria-label="Command palette"
        className="w-full max-w-xl overflow-hidden rounded-card border border-border bg-paper shadow-xl"
      >
        <input
          ref={input}
          aria-label="Type a command"
          placeholder="Type a command, service, project or port number"
          className="w-full border-b border-border bg-paper px-5 py-3 text-base outline-none"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setCursor(0);
          }}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') {
              e.preventDefault();
              setCursor((c) => Math.min(c + 1, shown.length - 1));
            } else if (e.key === 'ArrowUp') {
              e.preventDefault();
              setCursor((c) => Math.max(c - 1, 0));
            } else if (e.key === 'Enter') choose(shown[cursor]);
          }}
        />
        <ul role="listbox" className="max-h-80 overflow-auto py-1">
          {shown.length === 0 && <li className="px-5 py-3 text-sm text-fg-muted">No match.</li>}
          {shown.map((item, i) => (
            <li
              key={item.id}
              role="option"
              aria-selected={i === cursor}
              onMouseEnter={() => setCursor(i)}
              onClick={() => choose(item)}
              className={`cursor-pointer px-5 py-2 text-sm ${i === cursor ? 'bg-raised' : ''}`}
            >
              {item.title}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
