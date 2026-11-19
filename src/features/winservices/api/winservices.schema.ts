import { z } from 'zod';

export const WinServiceSchema = z.object({
  name: z.string(),
  displayName: z.string(),
  state: z.enum([
    'stopped',
    'startPending',
    'stopPending',
    'running',
    'continuePending',
    'pausePending',
    'paused',
  ]),
  startType: z.enum(['auto', 'autoDelayed', 'manual', 'disabled', 'boot', 'system']),
  serviceType: z.string(),
  pid: z.number().int().nullable(),
  binaryPath: z.string().nullable(),
  protected: z.boolean(),
  developer: z.boolean(),
});
export type WinService = z.infer<typeof WinServiceSchema>;
