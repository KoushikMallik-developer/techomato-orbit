import { describe, expect, it } from 'vitest';
import type { ServiceStatus } from '@/shared/ui/StatusDot/status';
import { layoutLevels, projectStatus, type DepEdge } from './dependencyGraph';

const svc = (...ids: string[]) => ids.map((id) => ({ id }));
const dep = (serviceId: string, dependsOnId: string): DepEdge => ({ serviceId, dependsOnId });

// The fixtures below are the same ones the Rust `engine::deps` tests use.
describe('layoutLevels agrees with the Rust order', () => {
  it('puts independent services in one level', () => {
    expect(layoutLevels(svc('x', 'y', 'z'), [])).toEqual([['x', 'y', 'z']]);
  });
  it('orders a chain dependencies first', () => {
    expect(layoutLevels(svc('c', 'b', 'a'), [dep('c', 'b'), dep('b', 'a')])).toEqual([
      ['a'],
      ['b'],
      ['c'],
    ]);
  });
  it('groups the middle of a diamond', () => {
    const edges = [dep('d', 'b'), dep('d', 'c'), dep('b', 'a'), dep('c', 'a')];
    expect(layoutLevels(svc('d', 'b', 'c', 'a'), edges)).toEqual([['a'], ['b', 'c'], ['d']]);
  });
  it('treats edges across projects as ordinary edges', () => {
    expect(layoutLevels(svc('web/ui', 'api/server'), [dep('web/ui', 'api/server')])).toEqual([
      ['api/server'],
      ['web/ui'],
    ]);
  });
  it('ignores dependencies on services it was not given', () => {
    expect(layoutLevels(svc('a'), [dep('a', 'gone')])).toEqual([['a']]);
  });
  it('never loops forever on a cycle; the stuck services end up last', () => {
    const levels = layoutLevels(svc('ok', 'a', 'b'), [dep('a', 'b'), dep('b', 'a')]);
    expect(levels[0]).toEqual(['ok']);
    expect(levels.flat().sort()).toEqual(['a', 'b', 'ok']);
  });
});

describe('projectStatus', () => {
  const s = (...x: ServiceStatus[]) => projectStatus(x);
  it('is running only when every service is running', () => {
    expect(s('running', 'running')).toBe('running');
  });
  it('is partial when some are up', () => {
    expect(s('running', 'stopped')).toBe('partial');
    expect(s('starting', 'stopped')).toBe('partial');
    expect(s('running', 'unresponsive')).toBe('partial');
  });
  it('is failed when any service failed, even if others run', () => {
    expect(s('running', 'failed')).toBe('failed');
    expect(s('failed', 'stopped')).toBe('failed');
  });
  it('is stopped when nothing is up, and for an empty project', () => {
    expect(s('stopped', 'stopped')).toBe('stopped');
    expect(s()).toBe('stopped');
  });
});
