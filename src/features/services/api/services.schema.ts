import { z } from 'zod';

export const CATEGORIES = [
  'database',
  'cache',
  'backend',
  'frontend',
  'worker',
  'queue',
  'custom',
] as const;

export const EnvVarSchema = z.object({
  key: z.string(),
  value: z.string().nullable(),
  isSecret: z.boolean(),
});

export const RestartPolicySchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('never') }),
  z.object({
    kind: z.literal('onFailure'),
    maxRetries: z.number().int(),
    backoffSecs: z.number().int(),
  }),
  z.object({
    kind: z.literal('always'),
    maxRetries: z.number().int(),
    backoffSecs: z.number().int(),
  }),
]);

export const HealthCheckSchema = z.discriminatedUnion('kind', [
  z.object({
    kind: z.literal('tcp'),
    port: z.number().int(),
    intervalSecs: z.number().int(),
    timeoutSecs: z.number().int(),
  }),
  z.object({
    kind: z.literal('http'),
    url: z.string(),
    minStatus: z.number().int(),
    maxStatus: z.number().int(),
    intervalSecs: z.number().int(),
    timeoutSecs: z.number().int(),
  }),
  z.object({
    kind: z.literal('command'),
    exe: z.string(),
    args: z.array(z.string()),
    intervalSecs: z.number().int(),
    timeoutSecs: z.number().int(),
  }),
]);

export const ServiceSpecSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  category: z.enum(CATEGORIES),
  projectId: z.string().nullable(),
  workingDir: z.string(),
  executable: z.string(),
  args: z.array(z.string()),
  runThroughShell: z.boolean(),
  env: z.array(EnvVarSchema),
  port: z.number().int().nullable(),
  startupTimeoutSecs: z.number().int(),
  shutdownTimeoutSecs: z.number().int(),
  restart: RestartPolicySchema,
  health: HealthCheckSchema.nullable(),
  keepRunningOnQuit: z.boolean(),
  enabled: z.boolean(),
});

export type ServiceSpec = z.infer<typeof ServiceSpecSchema>;
export type EnvVar = z.infer<typeof EnvVarSchema>;
export type Category = (typeof CATEGORIES)[number];
