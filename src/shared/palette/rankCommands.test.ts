import { describe, expect, it } from 'vitest';
import { rankCommands, type Command } from './rankCommands';

const c = (id: string, title: string, keywords?: string[]): Command => ({ id, title, keywords });
const list = [
  c('a', 'Go to Ports'),
  c('b', 'Start Redis'),
  c('c', 'Restart Postgres'),
  c('d', 'Toggle theme', ['dark', 'light']),
  c('e', 'Open settings'),
];
const ids = (q: string) => rankCommands(list, q).map((x) => x.id);

describe('rankCommands', () => {
  it('keeps registration order for an empty query', () => {
    expect(ids('')).toEqual(['a', 'b', 'c', 'd', 'e']);
    expect(ids('   ')).toEqual(['a', 'b', 'c', 'd', 'e']);
  });
  it('ranks a prefix above a later match above a subsequence', () => {
    // "st": prefix of "Start Redis", word-prefix in "Restart Postgres" (contains), subsequence in "Open settings"
    expect(ids('st').slice(0, 2)).toEqual(['b', 'c']);
  });
  it('matches subsequences, case-insensitively, and drops non-matches', () => {
    expect(ids('gtp')).toEqual(['a']);
    expect(ids('PORTS')).toEqual(['a']);
    expect(ids('zzz')).toEqual([]);
  });
  it('matches keywords, but a title match beats a keyword match', () => {
    expect(ids('dark')).toEqual(['d']);
    const two = rankCommands([c('k', 'Alpha', ['theme']), c('t', 'Theme')], 'theme');
    expect(two.map((x) => x.id)).toEqual(['t', 'k']);
  });
  it('is stable for equal scores', () => {
    const same = [c('1', 'Start one'), c('2', 'Start two'), c('3', 'Start three')];
    expect(rankCommands(same, 'start').map((x) => x.id)).toEqual(['1', '2', '3']);
  });
});
