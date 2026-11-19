import { z } from 'zod';

export const ManagerSchema = z.enum(['winget', 'choco', 'scoop']);
export type Manager = z.infer<typeof ManagerSchema>;

export const PackageSchema = z.object({
  id: z.string(),
  name: z.string(),
  version: z.string(),
  available: z.string().nullable(),
  source: z.string().nullable(),
  manager: ManagerSchema,
});
export type Package = z.infer<typeof PackageSchema>;

export const ManagerInfoSchema = z.object({ manager: ManagerSchema, available: z.boolean() });
export type ManagerInfo = z.infer<typeof ManagerInfoSchema>;

export const PmResultSchema = z.object({ ok: z.boolean(), code: z.number(), message: z.string() });
export type PmResult = z.infer<typeof PmResultSchema>;

export const OutputLineSchema = z.object({
  opId: z.string(),
  manager: ManagerSchema,
  line: z.string(),
});

export const MANAGER_LABEL: Record<Manager, string> = {
  winget: 'winget',
  choco: 'Chocolatey',
  scoop: 'Scoop',
};
