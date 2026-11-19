import { useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { pushToast } from '@/app/uiSlice';
import { store } from '@/app/store';
import { CMD } from '@/shared/ipc/commands';
import { useIpcEvent } from '@/shared/ipc/events';
import { ipc } from '@/shared/ipc/invoke';
import { tail } from '../packageFilter';
import {
  ManagerInfoSchema,
  OutputLineSchema,
  PackageSchema,
  PmResultSchema,
  type Manager,
} from './packages.schema';

export type View = 'installed' | 'outdated' | 'search';

export function useManagers() {
  return useQuery({
    queryKey: ['pm', 'managers'],
    queryFn: () => ipc(CMD.pmManagers, {}, z.array(ManagerInfoSchema)),
    staleTime: 60_000,
  });
}

/** One list for a manager. Search only runs once the user has submitted text. */
export function usePackageList(manager: Manager | undefined, view: View, query: string) {
  return useQuery({
    queryKey: ['pm', 'list', manager, view, view === 'search' ? query : ''],
    enabled: !!manager && (view !== 'search' || query.trim().length > 0),
    staleTime: 60_000,
    retry: false,
    queryFn: () => {
      const schema = z.array(PackageSchema);
      if (view === 'installed') return ipc(CMD.pmInstalled, { manager }, schema);
      if (view === 'outdated') return ipc(CMD.pmOutdated, { manager }, schema);
      return ipc(CMD.pmSearch, { manager, query }, schema);
    },
  });
}

type Change =
  | { kind: 'install' | 'upgrade' | 'uninstall'; manager: Manager; id: string; label: string }
  | { kind: 'upgradeAll'; manager: Manager; label: string };

export type Run = {
  label: string;
  lines: string[];
  running: boolean;
  ok: boolean | null;
};

const MAX_LINES = 500;

/** Runs one package change at a time, collecting its streamed output for the drawer. */
export function usePackageRun() {
  const qc = useQueryClient();
  const [run, setRun] = useState<Run | null>(null);
  const current = useRef<string | null>(null);

  useIpcEvent('pm://output', OutputLineSchema, (e) => {
    if (e.opId !== current.current) return;
    setRun((r) => (r ? { ...r, lines: tail(r.lines, e.line, MAX_LINES) } : r));
  });

  const mutation = useMutation({
    mutationFn: async (c: Change) => {
      const opId = crypto.randomUUID();
      current.current = opId;
      setRun({ label: c.label, lines: [], running: true, ok: null });
      const base = { manager: c.manager, opId };
      switch (c.kind) {
        case 'upgradeAll':
          return ipc(CMD.pmUpgradeAll, base, PmResultSchema);
        case 'install':
          return ipc(CMD.pmInstall, { ...base, id: c.id }, PmResultSchema);
        case 'upgrade':
          return ipc(CMD.pmUpgrade, { ...base, id: c.id }, PmResultSchema);
        case 'uninstall':
          return ipc(CMD.pmUninstall, { ...base, id: c.id }, PmResultSchema);
      }
    },
    onSuccess: (r) => {
      setRun((x) => (x ? { ...x, running: false, ok: r.ok } : x));
      if (!r.ok) store.dispatch(pushToast(r.message || 'The package manager reported an error.'));
    },
    onError: () => setRun(null), // refused before starting (cancelled, invalid, busy): the toast says why
    onSettled: () => qc.invalidateQueries({ queryKey: ['pm', 'list'] }),
  });

  return { run, start: mutation.mutate, busy: mutation.isPending, close: () => setRun(null) };
}
