import type { Package } from './api/packages.schema';

export function filterPackages(rows: Package[], text: string): Package[] {
  const q = text.trim().toLowerCase();
  if (!q) return rows;
  return rows.filter((r) => r.name.toLowerCase().includes(q) || r.id.toLowerCase().includes(q));
}

/** Mirrors the id allowlist in Rust (`pm::args::check_id`); other ids would be refused there. */
export function isManageable(id: string): boolean {
  return id.length <= 200 && !id.startsWith('-') && /^[A-Za-z0-9._+@/:-]+$/.test(id);
}

/** Appends a line and keeps only the newest `max`. */
export function tail(lines: string[], line: string, max: number): string[] {
  const next = [...lines, line];
  return next.length > max ? next.slice(next.length - max) : next;
}
