import { beforeEach, describe, expect, it } from 'vitest';
import type { Finding } from './api/doctor.schema';
import { addFinding, begin, failScan, finish, getState, resetForTest } from './doctorStore';

const f = (id: string, title = id): Finding => ({
  id,
  category: 'disk',
  title,
  detail: '',
  tag: 'info',
  severity: 'ok',
  fix: null,
});

beforeEach(resetForTest);

describe('doctor store', () => {
  it('refuses to start a second scan while one is running', () => {
    expect(begin()).toBe(true);
    expect(begin()).toBe(false);
    finish(10);
    expect(begin()).toBe(true);
  });
  it('replaces a finding with the same id and keeps results after the scan ends', () => {
    begin();
    addFinding(f('a', 'old'));
    addFinding(f('a', 'new'));
    addFinding(f('b'));
    finish(1234);
    const s = getState();
    expect(s.findings.map((x) => x.title)).toEqual(['new', 'b']);
    expect([s.scanning, s.millis]).toEqual([false, 1234]);
  });
  it('a new scan starts from an empty list', () => {
    begin();
    addFinding(f('a'));
    finish(1);
    begin();
    expect(getState().findings).toEqual([]);
  });
  it('a failed scan stops the spinner but keeps what was found', () => {
    begin();
    addFinding(f('a'));
    failScan();
    expect(getState()).toMatchObject({ scanning: false, findings: [expect.anything()] });
  });
});
