import { describe, expect, it } from 'vitest';
import { filterLines, type LogLine } from './logFilter';

const lines: LogLine[] = [
  { tsMs: 1, stream: 'stdout', text: 'Server listening on 8080' },
  { tsMs: 2, stream: 'stderr', text: 'ERROR: connection refused' },
  { tsMs: 3, stream: 'stdout', text: 'request GET /health' },
];
const both = new Set(['stdout', 'stderr'] as const);

describe('filterLines', () => {
  it('matches case-insensitively', () => {
    expect(filterLines(lines, 'error', both).map((l) => l.tsMs)).toEqual([2]);
    expect(filterLines(lines, 'SERVER', both).map((l) => l.tsMs)).toEqual([1]);
  });
  it('an empty query keeps every line of the chosen streams', () => {
    expect(filterLines(lines, '', both)).toHaveLength(3);
    expect(filterLines(lines, '', new Set(['stderr'] as const)).map((l) => l.tsMs)).toEqual([2]);
  });
  it('no streams selected shows nothing', () => {
    expect(filterLines(lines, '', new Set())).toEqual([]);
  });
  it('treats the query literally, not as a pattern', () => {
    expect(filterLines(lines, '.*', both)).toEqual([]);
    expect(filterLines(lines, 'GET /health', both)).toHaveLength(1);
  });
});
