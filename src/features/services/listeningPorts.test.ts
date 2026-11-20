import { describe, expect, it } from 'vitest';
import type { PortRow } from '@/features/ports/api/ports.schema';
import { listeningPorts } from './listeningPorts';

const row = (port: number, o: Partial<PortRow> = {}): PortRow => ({
  proto: 'tcp',
  localAddr: '127.0.0.1',
  port,
  state: 'LISTEN',
  pid: 1,
  createdAtMs: 1,
  processName: 'node.exe',
  path: null,
  serviceId: 'a',
  serviceName: 'Web',
  conflictsWith: null,
  ...o,
});

describe('listeningPorts', () => {
  it('counts a port open on IPv4 and IPv6 once, and ignores UDP and other services', () => {
    const rows = [
      row(5173),
      row(5173, { localAddr: '::1' }),
      row(5353, { proto: 'udp', state: null }),
      row(8080, { serviceId: 'b' }),
      row(9000, { state: 'ESTABLISHED' }),
    ];
    expect(listeningPorts(rows, 'a', null)).toEqual([5173]);
  });

  it('shows only the configured port when it is among the helper ports', () => {
    const rows = [row(3000), row(9229), row(51234)];
    expect(listeningPorts(rows, 'a', 3000)).toEqual([3000]);
    expect(listeningPorts(rows, 'a', 4000)).toEqual([3000, 9229, 51234]);
    expect(listeningPorts(undefined, 'a', null)).toEqual([]);
  });
});
