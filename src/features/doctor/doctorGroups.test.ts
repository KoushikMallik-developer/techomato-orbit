import { describe, expect, it } from 'vitest';
import type { Finding } from './api/doctor.schema';
import { attentionCount, groupFindings, upsert } from './doctorGroups';

const f = (o: Partial<Finding>): Finding => ({
  id: 'x',
  category: 'disk',
  title: 'T',
  detail: '',
  tag: 'info',
  severity: 'ok',
  fix: null,
  ...o,
});

describe('groupFindings', () => {
  it('orders groups by importance and rows by severity then title', () => {
    const groups = groupFindings([
      f({ id: 'a', category: 'tools', title: 'WSL', severity: 'info' }),
      f({ id: 'b', category: 'disk', title: 'D:', severity: 'ok' }),
      f({ id: 'c', category: 'disk', title: 'C:', severity: 'critical' }),
      f({ id: 'd', category: 'memory', title: 'Memory', severity: 'warn' }),
    ]);
    expect(groups.map((g) => g.category)).toEqual(['disk', 'memory', 'tools']);
    expect(groups[0]!.items.map((i) => i.id)).toEqual(['c', 'b']);
    expect(groups[0]!.worst).toBe('critical');
  });
  it('puts an unknown category last instead of dropping it', () => {
    const groups = groupFindings([f({ id: 'z', category: 'mystery' })]);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.category).toBe('mystery');
  });
});

describe('attentionCount', () => {
  it('counts only warnings and critical findings', () => {
    expect(
      attentionCount([
        f({ id: '1', severity: 'warn' }),
        f({ id: '2', severity: 'critical' }),
        f({ id: '3', severity: 'info' }),
        f({ id: '4', severity: 'ok' }),
      ]),
    ).toBe(2);
  });
});

describe('upsert', () => {
  it('replaces a finding with the same id so a rescan does not duplicate rows', () => {
    const a = f({ id: 'a', title: 'old' });
    const next = upsert([a, f({ id: 'b' })], f({ id: 'a', title: 'new' }));
    expect(next).toHaveLength(2);
    expect(next.find((x) => x.id === 'a')!.title).toBe('new');
  });
});
