import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { z } from 'zod';

/** Subscribes to a Rust event until the returned function is called. Payloads that fail the schema
 * are dropped with a console warning. */
export async function onIpcEvent<T>(
  name: string,
  schema: z.ZodType<T>,
  handler: (v: T) => void,
): Promise<() => void> {
  return listen<unknown>(name, (e) => {
    const parsed = schema.safeParse(e.payload);
    if (parsed.success) handler(parsed.data);
    else console.warn(`Dropped malformed ${name} event`, parsed.error.message);
  });
}

/** Subscribes while a component is mounted. */
export function useIpcEvent<T>(name: string, schema: z.ZodType<T>, handler: (v: T) => void) {
  useEffect(() => {
    let off: (() => void) | undefined;
    let disposed = false;
    void onIpcEvent(name, schema, handler).then((un) => (disposed ? un() : (off = un)));
    return () => {
      disposed = true;
      off?.();
    };
    // handler and schema are expected to be stable per subscription.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
  }, [name]);
}
