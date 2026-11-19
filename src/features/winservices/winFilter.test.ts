import { describe, expect, it } from 'vitest';
import type { WinService } from './api/winservices.schema';
import { filterWinServices, winActions } from './winFilter';

const svc = (o: Partial<WinService>): WinService => ({
  name: 'Redis',
  displayName: 'Redis Server',
  state: 'running',
  startType: 'auto',
  serviceType: 'Own process',
  pid: 10,
  binaryPath: 'C:\redis\redis.exe',
  protected: false,
  developer: true,
  ...o,
});

const list = [
  svc({}),
  svc({ name: 'pg', displayName: 'PostgreSQL', state: 'stopped', startType: 'manual', pid: null }),
  svc({ name: 'Spooler', displayName: 'Print Spooler', developer: false }),
];

describe('filterWinServices', () => {
  it('shows only developer services unless showAll', () => {
    expect(filterWinServices(list, '', 'all', 'all', false).map((s) => s.name)).toEqual([
      'Redis',
      'pg',
    ]);
    expect(filterWinServices(list, '', 'all', 'all', true)).toHaveLength(3);
  });
  it('matches name, display name and pid, case-insensitively', () => {
    expect(filterWinServices(list, 'postgre', 'all', 'all', true)).toHaveLength(1);
    expect(filterWinServices(list, 'SPOOL', 'all', 'all', true)).toHaveLength(1);
    expect(filterWinServices(list, '10', 'all', 'all', true)).toHaveLength(2);
  });
  it('treats the query literally, not as a pattern', () => {
    expect(filterWinServices(list, '.*', 'all', 'all', true)).toHaveLength(0);
  });
  it('filters by state and start type', () => {
    expect(filterWinServices(list, '', 'stopped', 'all', true).map((s) => s.name)).toEqual(['pg']);
    expect(filterWinServices(list, '', 'all', 'manual', true).map((s) => s.name)).toEqual(['pg']);
  });
});

describe('winActions', () => {
  it('offers nothing for protected services', () => {
    expect(winActions('running', true)).toEqual({ start: false, stop: false, restart: false });
    expect(winActions('stopped', true)).toEqual({ start: false, stop: false, restart: false });
  });
  it('running offers stop and restart', () => {
    expect(winActions('running', false)).toEqual({ start: false, stop: true, restart: true });
  });
  it('stopped offers start', () => {
    expect(winActions('stopped', false)).toEqual({ start: true, stop: false, restart: false });
  });
  it('pending and paused states offer nothing', () => {
    for (const s of [
      'startPending',
      'stopPending',
      'continuePending',
      'pausePending',
      'paused',
    ] as const)
      expect(winActions(s, false)).toEqual({ start: false, stop: false, restart: false });
  });
});
