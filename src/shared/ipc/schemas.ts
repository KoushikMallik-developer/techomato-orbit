import { z } from 'zod';

export const IpcErrorSchema = z.object({
  kind: z.string(),
  message: z.string(),
  detail: z.string().nullish(),
});
