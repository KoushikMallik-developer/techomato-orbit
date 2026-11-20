import { useState } from 'react';
import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { useIpcEvent } from '@/shared/ipc/events';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';
import { servicesKey } from '@/features/services/api/useServices';
import { ServiceSpecSchema } from '@/features/services/api/services.schema';

export const EngineSchema = z.object({
  id: z.string(),
  name: z.string(),
  version: z.string(),
  bytes: z.number(),
  defaultPort: z.number().int(),
  installed: z.boolean(),
});
export type Engine = z.infer<typeof EngineSchema>;

const ProgressSchema = z.object({
  engine: z.string(),
  phase: z.string(),
  done: z.number(),
  total: z.number(),
});
export type Progress = z.infer<typeof ProgressSchema>;

export const enginesKey = ['db-engines'] as const;
const createKey = ['db-create'] as const;

export function useEngines() {
  return useQuery({
    queryKey: enginesKey,
    queryFn: () => ipc(CMD.dbEngines, {}, z.array(EngineSchema)),
  });
}

/** Creates a database service (downloading the engine first when needed) and tracks its progress. */
export function useCreateDatabase() {
  const qc = useQueryClient();
  const [progress, setProgress] = useState<Progress | null>(null);
  useIpcEvent('engine://progress', ProgressSchema, setProgress);
  const create = useMutation({
    mutationKey: createKey,
    mutationFn: (v: { engine: string; name: string }) => ipc(CMD.dbCreate, v, ServiceSpecSchema),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: servicesKey });
      void qc.invalidateQueries({ queryKey: enginesKey });
    },
    onSettled: () => setProgress(null),
  });
  const cancel = () => void ipcVoid(CMD.dbCancel, {});
  // Also true after leaving this screen and coming back while a setup is still running.
  const running = useIsMutating({ mutationKey: createKey }) > 0;
  return { create, progress, cancel, busy: running };
}

export const megabytes = (b: number) => `${Math.round(b / (1024 * 1024))} MB`;
