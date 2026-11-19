import type { ServiceStatus } from '@/shared/ui/StatusDot/status';

export type Actions = { start: boolean; stop: boolean; restart: boolean; forceStop: boolean };

/** Which lifecycle buttons make sense for a status (the supervisor still enforces it). */
export function availableActions(status: ServiceStatus): Actions {
  switch (status) {
    case 'running':
      return { start: false, stop: true, restart: true, forceStop: true };
    case 'starting':
    case 'stopping':
      return { start: false, stop: false, restart: false, forceStop: true };
    case 'failed':
    case 'unresponsive':
      return { start: true, stop: false, restart: true, forceStop: true };
    case 'stopped':
    case 'unknown':
      return { start: true, stop: false, restart: false, forceStop: false };
  }
}
