import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { CMD, type CmdName } from '@/shared/ipc/commands';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';
import { WinServiceSchema } from './winservices.schema';

const listKey = ['winservices'] as const;
const pinsKey = ['winservices', 'pins'] as const;

/** Always fetches every service; the page filters, so the toggle is instant. */
export function useWinServices() {
  return useQuery({
    queryKey: listKey,
    queryFn: () => ipc(CMD.winserviceList, { all: true }, z.array(WinServiceSchema)),
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
  });
}

export function usePins() {
  const qc = useQueryClient();
  const pins = useQuery({
    queryKey: pinsKey,
    queryFn: () => ipc(CMD.winservicePins, {}, z.array(z.string())),
  });
  const set = useMutation({
    mutationFn: (names: string[]) => ipcVoid(CMD.winserviceSetPins, { names }),
    onSettled: () => qc.invalidateQueries({ queryKey: ['winservices'] }),
  });
  return { pins: pins.data ?? [], setPins: set.mutate };
}

export type WinControl = 'start' | 'stop' | 'restart';
const COMMAND: Record<WinControl, CmdName> = {
  start: CMD.winserviceStart,
  stop: CMD.winserviceStop,
  restart: CMD.winserviceRestart,
};

export function useWinControl() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { name: string; action: WinControl }) =>
      ipcVoid(COMMAND[v.action], { name: v.name }),
    onSettled: () => qc.invalidateQueries({ queryKey: listKey }),
  });
}
