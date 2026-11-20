import type { PortRow } from '@/features/ports/api/ports.schema';

/**
 * Ports a service's processes listen on (IPv4 and IPv6 count once). Dev tools open helper ports
 * too (a debugger, workers), so when the configured port is among them only it is shown.
 */
export function listeningPorts(
  ports: PortRow[] | undefined,
  id: string,
  configured: number | null,
): number[] {
  const mine = ports?.filter(
    (p) => p.serviceId === id && p.proto === 'tcp' && p.state === 'LISTEN',
  );
  const live = [...new Set(mine?.map((p) => p.port))].sort((a, b) => a - b);
  return configured !== null && live.includes(configured) ? [configured] : live;
}
