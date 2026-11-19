import type { ServiceStatus } from '@/shared/ui/StatusDot/status';

export type DepEdge = { serviceId: string; dependsOnId: string };

/** Start levels: level 0 depends on nothing. Mirrors `engine::deps::order` in Rust. */
export function layoutLevels(services: { id: string }[], deps: DepEdge[]): string[][] {
  const known = new Set(services.map((s) => s.id));
  const needs = new Map(services.map((s) => [s.id, new Set<string>()]));
  for (const d of deps) if (known.has(d.dependsOnId)) needs.get(d.serviceId)?.add(d.dependsOnId);
  const placed = new Set<string>();
  const levels: string[][] = [];
  while (placed.size < known.size) {
    const level = services
      .map((s) => s.id)
      .filter((id) => !placed.has(id) && [...(needs.get(id) ?? [])].every((d) => placed.has(d)));
    // Rust rejects cycles, so this only guards against looping on bad input.
    const next = level.length > 0 ? level : [...known].filter((id) => !placed.has(id));
    next.forEach((id) => placed.add(id));
    levels.push(next);
  }
  return levels;
}

export type ProjectStatus = 'running' | 'partial' | 'failed' | 'stopped';

/** One word for a whole project from its services' statuses. */
export function projectStatus(statuses: ServiceStatus[]): ProjectStatus {
  if (statuses.includes('failed')) return 'failed';
  if (statuses.length > 0 && statuses.every((s) => s === 'running')) return 'running';
  return statuses.some((s) => s !== 'stopped' && s !== 'unknown') ? 'partial' : 'stopped';
}
