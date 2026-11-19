/** Every Tauri command name. No raw command strings anywhere else. */
export const CMD = {
  serviceList: 'service_list',
  serviceGet: 'service_get',
  serviceCreate: 'service_create',
  serviceUpdate: 'service_update',
  serviceDelete: 'service_delete',
  serviceStart: 'service_start',
  serviceStop: 'service_stop',
  serviceForceStop: 'service_force_stop',
  serviceRestart: 'service_restart',
  serviceRuntimeList: 'service_runtime_list',
} as const;

export type CmdName = (typeof CMD)[keyof typeof CMD];
