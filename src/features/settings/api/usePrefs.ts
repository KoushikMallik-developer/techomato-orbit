import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';

export const PrefsSchema = z.object({
  modules: z.object({
    projects: z.boolean(),
    doctor: z.boolean(),
  }),
  notify: z.object({
    started: z.boolean(),
    stopped: z.boolean(),
    crashed: z.boolean(),
    startFailed: z.boolean(),
    healthFailed: z.boolean(),
    autoRestart: z.boolean(),
  }),
  refreshHidden: z.boolean(),
  startWithWindows: z.boolean(),
  autoUpdateCheck: z.boolean(),
});
export type Prefs = z.infer<typeof PrefsSchema>;

const prefsKey = ['prefs'] as const;

export function usePrefs() {
  return useQuery({
    queryKey: prefsKey,
    queryFn: () => ipc(CMD.prefsGet, {}, PrefsSchema),
    staleTime: Infinity,
  });
}

/** Saves the whole document; the screen updates at once and rolls back if the save fails. */
export function useSetPrefs() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (prefs: Prefs) => ipcVoid(CMD.prefsSet, { prefs }),
    onMutate: async (prefs) => {
      const before = qc.getQueryData<Prefs>(prefsKey);
      qc.setQueryData(prefsKey, prefs);
      return { before };
    },
    onError: (_e, _v, ctx) => qc.setQueryData(prefsKey, ctx?.before),
  });
}

const SHORTCUTS = z.array(
  z.object({ label: z.string(), accelerator: z.string(), error: z.string().nullable() }),
);

export function useShortcuts() {
  return useQuery({
    queryKey: ['shortcuts'],
    queryFn: () => ipc(CMD.shortcutsStatus, {}, SHORTCUTS),
    staleTime: Infinity,
  });
}

export function useAutostart() {
  const qc = useQueryClient();
  const query = useQuery({
    queryKey: ['autostart'],
    queryFn: () => ipc(CMD.autostartGet, {}, z.boolean()),
  });
  const set = useMutation({
    mutationFn: (enabled: boolean) => ipcVoid(CMD.autostartSet, { enabled }),
    onSettled: () => qc.invalidateQueries({ queryKey: ['autostart'] }),
  });
  return { enabled: query.data ?? false, set: set.mutate };
}

/** True while the window is on screen; polling pauses when it is hidden to the tray. */
export function useVisible(): boolean {
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible');
  useEffect(() => {
    const on = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', on);
    return () => document.removeEventListener('visibilitychange', on);
  }, []);
  return visible;
}

/** `ms` while visible (or when the user allows background refresh), otherwise no polling. */
export function usePollInterval(ms: number): number | false {
  const visible = useVisible();
  const prefs = usePrefs().data;
  return visible || prefs?.refreshHidden ? ms : false;
}
