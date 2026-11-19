import { describe, expect, it } from 'vitest';
import type { Package } from './api/packages.schema';
import { filterPackages, isManageable, tail } from './packageFilter';

const p = (o: Partial<Package>): Package => ({
  id: 'Git.Git',
  name: 'Git',
  version: '2.0',
  available: null,
  source: 'winget',
  manager: 'winget',
  ...o,
});

describe('filterPackages', () => {
  const rows = [p({}), p({ id: 'OpenAI.ChatGPT', name: 'ChatGPT' }), p({ id: 'x.y', name: 'Zed' })];
  it('matches name or id, ignoring case and surrounding spaces', () => {
    expect(filterPackages(rows, ' chat ').map((r) => r.id)).toEqual(['OpenAI.ChatGPT']);
    expect(filterPackages(rows, 'X.Y').map((r) => r.name)).toEqual(['Zed']);
  });
  it('returns everything for an empty filter', () => {
    expect(filterPackages(rows, '  ')).toHaveLength(3);
  });
});

describe('isManageable', () => {
  it('accepts ids the backend accepts', () => {
    for (const id of ['Git.Git', 'git-lfs', 'a+b', 'pkg@1.2', 'bucket/app', 'x:y']) {
      expect(isManageable(id), id).toBe(true);
    }
  });
  it('rejects ids the backend refuses, such as Add/Remove Programs entries', () => {
    for (const id of [
      '',
      'ARP\\Machine\\X64\\Android Studio',
      'a b',
      '-x',
      'a&b',
      'a'.repeat(201),
    ]) {
      expect(isManageable(id), id).toBe(false);
    }
  });
});

describe('tail', () => {
  it('keeps only the newest lines', () => {
    expect(tail(['a', 'b', 'c'], 'd', 3)).toEqual(['b', 'c', 'd']);
    expect(tail([], 'a', 3)).toEqual(['a']);
  });
});
