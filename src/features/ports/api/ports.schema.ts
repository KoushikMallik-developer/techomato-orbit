import { z } from 'zod';

export const PortRowSchema = z.object({
  proto: z.enum(['tcp', 'udp']),
  localAddr: z.string(),
  port: z.number().int(),
  state: z.string().nullable(),
  pid: z.number().int(),
  createdAtMs: z.number().nullable(),
  processName: z.string().nullable(),
  path: z.string().nullable(),
  serviceId: z.string().nullable(),
  serviceName: z.string().nullable(),
  conflictsWith: z.string().nullable(),
});
export type PortRow = z.infer<typeof PortRowSchema>;
