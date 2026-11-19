import { useCallback, useEffect, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';
import { usePollInterval } from '@/features/settings/api/usePrefs';
import { PortRowSchema, type PortRow } from './ports.schema';

export const portsKey = ['ports'] as const;

/** Polls every 2 s while the page is mounted and the window is visible. */
export function usePorts() {
  const interval = usePollInterval(2_000);
  return useQuery({
    queryKey: portsKey,
    queryFn: () => ipc(CMD.portsList, {}, z.array(PortRowSchema)),
    refetchInterval: interval,
  });
}

export function useCheckPort() {
  return useMutation({
    mutationFn: (port: number) => ipc(CMD.portCheck, { port }, z.boolean()),
  });
}

export function useEndProcess() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { row: PortRow; force: boolean }) =>
      ipcVoid(CMD.portEndProcess, {
        pid: v.row.pid,
        createdAtMs: v.row.createdAtMs,
        force: v.force,
      }),
    onSettled: () => qc.invalidateQueries({ queryKey: portsKey }),
  });
}

const WATCH_KEY = 'orbit.ports.watch';

function loadWatch(): number[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(WATCH_KEY) ?? '[]');
    return Array.isArray(v) ? v.filter((n): n is number => Number.isInteger(n)) : [];
  } catch {
    return [];
  }
}

/** Ports the user wants to be told about. Kept in this browser profile. */
export function useWatchList() {
  const [watch, setWatch] = useState<number[]>(loadWatch);
  const save = useCallback((next: number[]) => {
    setWatch(next);
    try {
      localStorage.setItem(WATCH_KEY, JSON.stringify(next));
    } catch {
      /* losing the list on restart is acceptable */
    }
  }, []);
  return {
    watch,
    add: (p: number) => !watch.includes(p) && save([...watch, p].sort((a, b) => a - b)),
    remove: (p: number) => save(watch.filter((x) => x !== p)),
  };
}

/**
 * Calls `onConflict` once each time a watched port becomes occupied by something that is not the
 * Orbit service configured for it (the page itself stays quiet for a service using its own port).
 */
export function useWatchAlerts(
  rows: PortRow[] | undefined,
  watch: number[],
  onConflict: (row: PortRow) => void,
) {
  const alerted = useRef(new Set<number>());
  useEffect(() => {
    if (!rows) return;
    for (const p of watch) {
      const hit = rows.find((r) => r.port === p && r.proto === 'tcp' && !r.serviceId);
      if (hit && !alerted.current.has(p)) {
        alerted.current.add(p);
        onConflict(hit);
      } else if (!hit) {
        alerted.current.delete(p);
      }
    }
  }, [rows, watch, onConflict]);
}
