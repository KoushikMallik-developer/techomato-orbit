import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { useIpcEvent } from '@/shared/ipc/events';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';

const UpdateSchema = z
  .object({ version: z.string(), current: z.string(), notes: z.string().nullable() })
  .nullable();
const ProgressSchema = z.object({ downloaded: z.number(), total: z.number().nullable() });

/** Manual update check and install. Nothing installs until the user presses Install and confirms. */
export function useUpdates() {
  const [percent, setPercent] = useState<number | null>(null);
  useIpcEvent('update://progress', ProgressSchema, (p) =>
    setPercent(p.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null),
  );
  const check = useMutation({
    mutationFn: () => ipc(CMD.updateCheck, {}, UpdateSchema),
  });
  const install = useMutation({
    mutationFn: () => ipcVoid(CMD.updateInstall),
    onSettled: () => setPercent(null),
  });
  return { check, install, percent };
}
