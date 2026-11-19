import { describe, expect, it } from 'vitest';
import type { PortRow } from './api/ports.schema';
import { canEnd, filterPorts, isProtected } from './portFilter';

const row = (o: Partial<PortRow>): PortRow => ({
  proto: 'tcp',
  localAddr: '127.0.0.1',
  port: 3000,
  state: 'LISTEN',
  pid: 100,
  createdAtMs: 1,
  processName: 'node.exe',
  path: 'C:/Program Files/nodejs/node.exe',
  serviceId: null,
  serviceName: null,
  conflictsWith: null,
  ...o,
});
const rows = [
  row({}),
  row({
    port: 5432,
    pid: 200,
    processName: 'postgres.exe',
    path: null,
    serviceId: 's',
    serviceName: 'Database',
  }),
  row({ port: 5353, proto: 'udp', state: null, pid: 300, processName: 'svchost.exe', path: null }),
];

describe('filterPorts', () => {
  it('keeps everything for an empty query', () => {
    expect(filterPorts(rows, '', 'all', false)).toHaveLength(3);
    expect(filterPorts(rows, '   ', 'all', false)).toHaveLength(3);
  });
  it('matches port, process, path and service name ignoring case', () => {
    expect(filterPorts(rows, '5432', 'all', false)).toHaveLength(1);
    expect(filterPorts(rows, 'NODE', 'all', false)).toHaveLength(1);
    expect(filterPorts(rows, 'program files', 'all', false)).toHaveLength(1);
    expect(filterPorts(rows, 'database', 'all', false)).toHaveLength(1);
  });
  it('rows with an unreadable path still match by port or pid', () => {
    expect(filterPorts(rows, '5353', 'all', false).map((r) => r.pid)).toEqual([300]);
    expect(filterPorts(rows, '200', 'all', false).map((r) => r.port)).toEqual([5432]);
  });
  it('treats the query literally, not as a pattern', () => {
    expect(filterPorts(rows, '.*', 'all', false)).toHaveLength(0);
    expect(filterPorts(rows, '(', 'all', false)).toHaveLength(0);
  });
  it('filters by protocol', () => {
    expect(filterPorts(rows, '', 'udp', false).map((r) => r.port)).toEqual([5353]);
    expect(filterPorts(rows, '', 'tcp', false)).toHaveLength(2);
  });
  it('only mine keeps rows owned by an Orbit service', () => {
    expect(filterPorts(rows, '', 'all', true).map((r) => r.port)).toEqual([5432]);
  });
  it('combines filters', () => {
    expect(filterPorts(rows, 'svchost', 'tcp', false)).toHaveLength(0);
  });
});

describe('isProtected', () => {
  it('hides End for System and Idle', () => {
    expect(isProtected(row({ pid: 4 }))).toBe(true);
    expect(isProtected(row({ pid: 0 }))).toBe(true);
    expect(isProtected(row({ pid: 100 }))).toBe(false);
  });
});

describe('canEnd', () => {
  it('offers End for an ordinary foreign process', () => {
    expect(canEnd(row({}))).toBe(true);
  });
  it('does not offer End for protected processes', () => {
    expect(canEnd(row({ pid: 4 }))).toBe(false);
  });
  it('leaves Orbit services to the Services page, where the supervisor stops them cleanly', () => {
    expect(canEnd(row({ serviceId: 'svc-1', serviceName: 'API' }))).toBe(false);
  });
});
