import { z } from 'zod';

export const TagSchema = z.enum(['speed', 'storage', 'info']);
export const SeveritySchema = z.enum(['ok', 'info', 'warn', 'critical']);
export type Severity = z.infer<typeof SeveritySchema>;

/** The closed set of fixes; mirrors `Fix` in src-tauri/src/doctor/mod.rs. */
export const FixSchema = z.discriminatedUnion('kind', [
  z.object({
    kind: z.literal('clearCache'),
    id: z.enum(['npm', 'nuget', 'pip', 'temp', 'chrome', 'edge']),
  }),
  z.object({ kind: z.literal('disableStartup'), name: z.string() }),
  z.object({ kind: z.literal('setPowerPlan'), guid: z.string() }),
]);
export type Fix = z.infer<typeof FixSchema>;

export const FindingSchema = z.object({
  id: z.string(),
  category: z.string(),
  title: z.string(),
  detail: z.string(),
  tag: TagSchema,
  severity: SeveritySchema,
  fix: FixSchema.nullable(),
});
export type Finding = z.infer<typeof FindingSchema>;

export const DoneSchema = z.object({ count: z.number(), millis: z.number() });
export const FixResultSchema = z.object({ message: z.string(), freedBytes: z.number() });
