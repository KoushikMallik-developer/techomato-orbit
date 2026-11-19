import { z } from 'zod';

export const ProjectSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  icon: z.string(),
  rootDir: z.string(),
  archived: z.boolean(),
});
export type Project = z.infer<typeof ProjectSchema>;

export const ON_FAILURE = ['abort', 'continue'] as const;
export type OnFailure = (typeof ON_FAILURE)[number];

export const EdgeSchema = z.object({
  serviceId: z.string(),
  dependsOnId: z.string(),
  onFailure: z.enum(ON_FAILURE),
});
export type Edge = z.infer<typeof EdgeSchema>;

const OutcomeSchema = z.discriminatedUnion('state', [
  z.object({ state: z.literal('started') }),
  z.object({ state: z.literal('alreadyRunning') }),
  z.object({ state: z.literal('stopped') }),
  z.object({ state: z.literal('notRunning') }),
  z.object({ state: z.literal('failed'), message: z.string() }),
  z.object({ state: z.literal('skipped'), reason: z.string() }),
]);

export const RunReportSchema = z.object({
  projectId: z.string(),
  services: z.array(z.object({ serviceId: z.string(), name: z.string(), outcome: OutcomeSchema })),
  warnings: z.array(z.string()),
});
export type RunReport = z.infer<typeof RunReportSchema>;

export const ProgressSchema = z.object({
  projectId: z.string(),
  serviceId: z.string(),
  step: z.string(),
});
export type Progress = z.infer<typeof ProgressSchema>;

/** Plain-language summary of problems in a run, or null when everything went fine. */
export function reportProblems(r: RunReport): string | null {
  const lines = r.services.flatMap((s) =>
    s.outcome.state === 'failed'
      ? [`${s.name} failed: ${s.outcome.message}`]
      : s.outcome.state === 'skipped'
        ? [`${s.name} was skipped: ${s.outcome.reason}`]
        : [],
  );
  return [...lines, ...r.warnings].join(' ') || null;
}
