import { useEffect, useRef, useState } from 'react';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { useIpcEvent } from '@/shared/ipc/events';
import { ipc } from '@/shared/ipc/invoke';
import type { LogLine } from '../logFilter';

/** The most lines kept (and so rendered) per service. */
export const MAX_LINES = 5000;

const LineSchema = z.object({
  tsMs: z.number(),
  stream: z.enum(['stdout', 'stderr']),
  text: z.string(),
});
const BatchSchema = z.array(LineSchema.extend({ serviceId: z.string() }));

/**
 * Recent output of one service: loaded once, then appended to by batched `log://line` events.
 * The buffer is bounded, so a chatty service cannot grow the UI's memory.
 */
export function useLogBuffer(serviceId: string | null) {
  const [lines, setLines] = useState<LogLine[]>([]);
  // The event handler is registered once, so it reads the current id from a ref.
  const idRef = useRef(serviceId);
  idRef.current = serviceId;

  useEffect(() => {
    setLines([]);
    if (!serviceId) return;
    let stale = false;
    void ipc(CMD.logRecent, { serviceId, n: MAX_LINES }, z.array(LineSchema))
      .then((recent) => {
        // Lines that arrived while loading are kept after the loaded ones.
        if (!stale) setLines((cur) => [...recent, ...cur].slice(-MAX_LINES));
      })
      .catch(() => undefined); // the service was deleted: nothing to show
    return () => {
      stale = true;
    };
  }, [serviceId]);

  useIpcEvent('log://line', BatchSchema, (batch) => {
    const mine = batch.filter((l) => l.serviceId === idRef.current);
    if (mine.length === 0) return;
    setLines((cur) =>
      [...cur, ...mine.map(({ tsMs, stream, text }) => ({ tsMs, stream, text }))].slice(-MAX_LINES),
    );
  });

  return { lines, clear: () => setLines([]) };
}
