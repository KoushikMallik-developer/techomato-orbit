import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { z } from 'zod';

/** Subscribes to a Rust event; payloads that fail the schema are dropped with a console warning. */
export function useIpcEvent<T>(name: string, schema: z.ZodType<T>, handler: (v: T) => void) {
  useEffect(() => {
    let off: (() => void) | undefined;
    let disposed = false;
    void listen<unknown>(name, (e) => {
      const parsed = schema.safeParse(e.payload);
      if (parsed.success) handler(parsed.data);
      else console.warn(`Dropped malformed ${name} event`, parsed.error.message);
    }).then((un) => (disposed ? un() : (off = un)));
    return () => {
      disposed = true;
      off?.();
    };
    // handler and schema are expected to be stable per subscription.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
  }, [name]);
}
