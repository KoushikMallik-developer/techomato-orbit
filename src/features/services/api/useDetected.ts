import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { ipc } from '@/shared/ipc/invoke';
import { usePollInterval } from '@/features/settings/api/usePrefs';
import { servicesKey } from './useServices';

export const FoundSchema = z.object({
  name: z.string(),
  port: z.number().int(),
  pid: z.number().int(),
});
export type Found = z.infer<typeof FoundSchema>;

const DetectedSchema = z.object({ added: z.number().int(), skipped: z.array(FoundSchema) });

/**
 * Looking is also adding: Rust turns every database already running on this PC into a service.
 * `skipped` are the ones it could not take over.
 */
export function useDetected() {
  const qc = useQueryClient();
  const interval = usePollInterval(10_000);
  const list = useQuery({
    queryKey: ['detected-services'],
    queryFn: () => ipc(CMD.serviceDetect, {}, DetectedSchema),
    refetchInterval: interval,
  });
  const added = list.data?.added ?? 0;
  useEffect(() => {
    if (added > 0) void qc.invalidateQueries({ queryKey: servicesKey });
  }, [added, list.dataUpdatedAt, qc]);
  return { skipped: list.data?.skipped ?? [] };
}
