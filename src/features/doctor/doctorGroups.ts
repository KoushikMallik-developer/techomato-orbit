import type { Finding, Severity } from './api/doctor.schema';

export const CATEGORY_ORDER = ['disk', 'cache', 'memory', 'startup', 'power', 'system', 'tools'];
export const CATEGORY_TITLE: Record<string, string> = {
  disk: 'Drives',
  cache: 'Caches',
  memory: 'Memory',
  startup: 'Startup',
  power: 'Power',
  system: 'System',
  tools: 'Dev tools',
};

const RANK: Record<Severity, number> = { ok: 0, info: 1, warn: 2, critical: 3 };

export type Group = { category: string; items: Finding[]; worst: Severity };

export function groupFindings(findings: Finding[]): Group[] {
  const by = new Map<string, Finding[]>();
  for (const f of findings) by.set(f.category, [...(by.get(f.category) ?? []), f]);
  const order = (c: string) => {
    const i = CATEGORY_ORDER.indexOf(c);
    return i === -1 ? CATEGORY_ORDER.length : i;
  };
  return [...by.entries()]
    .sort(([a], [b]) => order(a) - order(b) || a.localeCompare(b))
    .map(([category, items]) => {
      const sorted = [...items].sort(
        (a, b) => RANK[b.severity] - RANK[a.severity] || a.title.localeCompare(b.title),
      );
      return { category, items: sorted, worst: sorted[0]?.severity ?? 'ok' };
    });
}

/** Warnings and critical findings: what the user should look at. */
export function attentionCount(findings: Finding[]): number {
  return findings.filter((f) => f.severity === 'warn' || f.severity === 'critical').length;
}

export function upsert(list: Finding[], f: Finding): Finding[] {
  const i = list.findIndex((x) => x.id === f.id);
  if (i === -1) return [...list, f];
  const next = [...list];
  next[i] = f;
  return next;
}
