import { describe, expect, test } from 'vitest';
import { SERVICE_STATUSES, statusLabel, statusTone } from './status';

describe('statusLabel', () => {
  test('gives the exact label for every status', () => {
    expect(SERVICE_STATUSES.map(statusLabel)).toEqual([
      'Stopped',
      'Starting',
      'Running',
      'Stopping',
      'Failed',
      'Unresponsive',
      'Unknown',
    ]);
  });
});

describe('statusTone', () => {
  test('failed is danger and unresponsive is warn, so colour pairs with a label', () => {
    expect(statusTone('failed')).toBe('danger');
    expect(statusTone('unresponsive')).toBe('warn');
  });
  test('running is running and transitional states are starting', () => {
    expect(statusTone('running')).toBe('running');
    expect(statusTone('starting')).toBe('starting');
    expect(statusTone('stopping')).toBe('starting');
  });
  test('stopped and unknown are neutral', () => {
    expect(statusTone('stopped')).toBe('stopped');
    expect(statusTone('unknown')).toBe('stopped');
  });
});
