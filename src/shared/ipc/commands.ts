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
  serviceSecretIsSet: 'service_secret_is_set',
  logRecent: 'log_recent',
  logHistory: 'log_history',
  logClear: 'log_clear',
  logExport: 'log_export',
  portsList: 'ports_list',
  portCheck: 'port_check',
  portEndProcess: 'port_end_process',
  notificationList: 'notification_list',
  notificationClear: 'notification_clear',
  notifyPortConflict: 'notify_port_conflict',
  prefsGet: 'prefs_get',
  prefsSet: 'prefs_set',
  pickPath: 'pick_path',
} as const;

export type CmdName = (typeof CMD)[keyof typeof CMD];
