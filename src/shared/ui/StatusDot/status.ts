export const SERVICE_STATUSES = [
  'stopped',
  'starting',
  'running',
  'stopping',
  'failed',
  'unresponsive',
  'unknown',
] as const;

export type ServiceStatus = (typeof SERVICE_STATUSES)[number];
export type StatusTone = 'running' | 'stopped' | 'starting' | 'warn' | 'danger';

const LABELS: Record<ServiceStatus, string> = {
  stopped: 'Stopped',
  starting: 'Starting',
  running: 'Running',
  stopping: 'Stopping',
  failed: 'Failed',
  unresponsive: 'Unresponsive',
  unknown: 'Unknown',
};

const TONES: Record<ServiceStatus, StatusTone> = {
  stopped: 'stopped',
  starting: 'starting',
  running: 'running',
  stopping: 'starting',
  failed: 'danger',
  unresponsive: 'warn',
  unknown: 'stopped',
};

export const statusLabel = (s: ServiceStatus): string => LABELS[s];
export const statusTone = (s: ServiceStatus): StatusTone => TONES[s];
