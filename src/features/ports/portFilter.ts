import type { PortRow } from './api/ports.schema';

export type ProtoFilter = 'all' | 'tcp' | 'udp';

/** Case-insensitive literal match on port, pid, process, path and service name. */
export function filterPorts(
  rows: PortRow[],
  query: string,
  proto: ProtoFilter,
  onlyMine: boolean,
): PortRow[] {
  const q = query.trim().toLowerCase();
  return rows.filter((r) => {
    if (proto !== 'all' && r.proto !== proto) return false;
    if (onlyMine && !r.serviceId) return false;
    if (q === '') return true;
    return [String(r.port), String(r.pid), r.processName, r.path, r.serviceName].some((f) =>
      f?.toLowerCase().includes(q),
    );
  });
}

/** Rows Orbit will never offer to end (the backend enforces this too). */
export function isProtected(row: PortRow): boolean {
  return row.pid === 0 || row.pid === 4;
}

/** End is offered only for foreign processes; Orbit's own services are stopped from Services. */
export function canEnd(row: PortRow): boolean {
  return !isProtected(row) && row.serviceId === null;
}
