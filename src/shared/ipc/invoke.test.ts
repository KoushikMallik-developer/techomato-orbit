import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import { z } from 'zod';

// A plain function, not vi.fn: vitest's call tracking can leak an unhandled rejection for
// mocks that reject.
let reply: () => Promise<unknown> = () => Promise.resolve(undefined);
vi.mock('@tauri-apps/api/core', () => ({ invoke: () => reply() }));

import { IpcError, ipc, ipcVoid } from './invoke';

async function rejection(p: Promise<unknown>): Promise<IpcError> {
  try {
    await p;
  } catch (e) {
    return e as IpcError;
  }
  throw new Error('expected the call to reject');
}

describe('ipc', () => {
  test('returns data that matches the schema', async () => {
    reply = () => Promise.resolve({ n: 1 });
    await expect(ipc('service_list', {}, z.object({ n: z.number() }))).resolves.toEqual({ n: 1 });
  });

  test('a reply that fails the schema rejects with IpcError kind "schema"', async () => {
    reply = () => Promise.resolve({ n: 'nope' });
    const err = await rejection(ipc('service_list', {}, z.object({ n: z.number() })));
    expect(err).toBeInstanceOf(IpcError);
    expect(err.kind).toBe('schema');
  });

  test('a Rust-shaped error becomes IpcError with the same kind, message and detail', async () => {
    reply = () => Promise.reject({ kind: 'conflict', message: 'port 80 in use', detail: 'pid 4' });
    const err = await rejection(ipc('service_list', {}, z.unknown()));
    expect(err).toBeInstanceOf(IpcError);
    expect(err.kind).toBe('conflict');
    expect(err.message).toBe('port 80 in use');
    expect(err.detail).toBe('pid 4');
  });

  test('an unrecognised rejection becomes kind "internal"', async () => {
    reply = () => Promise.reject('boom');
    const err = await rejection(ipcVoid('service_list'));
    expect(err.kind).toBe('internal');
    expect(err.message).toBe('boom');
  });
});

describe('import boundary', () => {
  function walk(dir: string): string[] {
    return readdirSync(dir).flatMap((f) => {
      const p = join(dir, f);
      return statSync(p).isDirectory() ? walk(p) : [p];
    });
  }

  test('only invoke.ts imports @tauri-apps/api/core', () => {
    const offenders = walk('src')
      .filter((f) => /\.(ts|tsx)$/.test(f) && !f.endsWith('invoke.ts') && !f.endsWith('.test.ts'))
      .filter((f) => readFileSync(f, 'utf8').includes('@tauri-apps/api/core'));
    expect(offenders).toEqual([]);
  });
});
