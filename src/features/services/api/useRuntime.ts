import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { useIpcEvent } from '@/shared/ipc/events';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';
import { SERVICE_STATUSES } from '@/shared/ui/StatusDot/status';

export const RuntimeSchema = z.object({
  id: z.string(),
  status: z.enum(SERVICE_STATUSES),
  pid: z.number().int().nullable(),
  parentPid: z.number().int().nullable(),
  startedAtMs: z.number().nullable(),
  exitCode: z.number().int().nullable(),
  restartCount: z.number().int(),
  lastError: z.string().nullable(),
});
export type Runtime = z.infer<typeof RuntimeSchema>;

export const runtimeKey = ['runtime'] as const;
type RuntimeMap = Record<string, Runtime>;

/** Live state per service id: loaded once, then kept current by `service://status` events. */
export function useRuntime() {
  const qc = useQueryClient();
  const query = useQuery({
    queryKey: runtimeKey,
    queryFn: async (): Promise<RuntimeMap> => {
      const list = await ipc(CMD.serviceRuntimeList, {}, z.array(RuntimeSchema));
      return Object.fromEntries(list.map((r) => [r.id, r]));
    },
  });
  useIpcEvent('service://status', RuntimeSchema, (r) =>
    qc.setQueryData<RuntimeMap>(runtimeKey, (old) => ({ ...old, [r.id]: r })),
  );
  return query.data ?? {};
}

export function useServiceActions() {
  const useAction = (
    cmd:
      | typeof CMD.serviceStart
      | typeof CMD.serviceStop
      | typeof CMD.serviceRestart
      | typeof CMD.serviceForceStop,
  ) =>
    // The status event updates the UI; only failures need handling (the global toast does it).
    useMutation({ mutationFn: (id: string) => ipcVoid(cmd, { id }) });
  return {
    start: useAction(CMD.serviceStart),
    stop: useAction(CMD.serviceStop),
    restart: useAction(CMD.serviceRestart),
    forceStop: useAction(CMD.serviceForceStop),
  };
}
