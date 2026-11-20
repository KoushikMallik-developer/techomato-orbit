import { useQuery } from '@tanstack/react-query';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { ipc } from '@/shared/ipc/invoke';
import { usePollInterval } from '@/features/settings/api/usePrefs';
import { PortRowSchema } from './ports.schema';

/** What is listening, from the PC. The Services page uses it to show each service's real port. */
export function usePorts(enabled = true) {
  const interval = usePollInterval(2_000);
  return useQuery({
    queryKey: ['ports'],
    queryFn: () => ipc(CMD.portsList, {}, z.array(PortRowSchema)),
    refetchInterval: interval,
    enabled,
  });
}
