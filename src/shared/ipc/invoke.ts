// The ONLY module that imports @tauri-apps/api/core.
import { invoke } from '@tauri-apps/api/core';
import type { z } from 'zod';
import type { CmdName } from './commands';

export class IpcError extends Error {
  constructor(
    public kind: string,
    message: string,
    public detail?: string,
  ) {
    super(message);
    this.name = 'IpcError';
  }
}

type RustError = { kind: string; message: string; detail?: string | null };

function isRustError(e: unknown): e is RustError {
  return (
    typeof e === 'object' &&
    e !== null &&
    typeof (e as RustError).kind === 'string' &&
    typeof (e as RustError).message === 'string'
  );
}

function toIpcError(e: unknown): IpcError {
  if (e instanceof IpcError) return e;
  if (isRustError(e)) return new IpcError(e.kind, e.message, e.detail ?? undefined);
  return new IpcError('internal', e instanceof Error ? e.message : String(e));
}

/** Calls a command and validates the reply against `schema`. */
export async function ipc<T>(cmd: CmdName, args: unknown, schema: z.ZodType<T>): Promise<T> {
  let raw: unknown;
  try {
    raw = await invoke(cmd, args as Record<string, unknown>);
  } catch (e) {
    throw toIpcError(e);
  }
  const parsed = schema.safeParse(raw);
  if (!parsed.success) throw new IpcError('schema', `Bad reply from ${cmd}`, parsed.error.message);
  return parsed.data;
}

/** For mutations that return nothing. */
export async function ipcVoid(cmd: CmdName, args?: unknown): Promise<void> {
  try {
    await invoke(cmd, args as Record<string, unknown> | undefined);
  } catch (e) {
    throw toIpcError(e);
  }
}
