import { describe, expect, it } from 'vitest';
import { SERVICE_STATUSES } from '@/shared/ui/StatusDot/status';
import { availableActions } from './powerState';

const on = (...keys: string[]) => ({
  start: keys.includes('start'),
  stop: keys.includes('stop'),
  restart: keys.includes('restart'),
  forceStop: keys.includes('forceStop'),
});

describe('availableActions', () => {
  it.each([
    ['stopped', on('start')],
    ['unknown', on('start')],
    ['running', on('stop', 'restart', 'forceStop')],
    ['starting', on('forceStop')],
    ['stopping', on('forceStop')],
    ['failed', on('start', 'restart', 'forceStop')],
    ['unresponsive', on('start', 'restart', 'forceStop')],
  ] as const)('%s', (status, expected) => {
    expect(availableActions(status)).toEqual(expected);
  });

  it('covers every status', () => {
    for (const s of SERVICE_STATUSES) expect(availableActions(s)).toBeDefined();
  });
});
