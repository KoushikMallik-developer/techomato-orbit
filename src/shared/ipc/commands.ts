/** Every Tauri command name. No raw command strings anywhere else. */
export const CMD = {
  serviceList: 'service_list',
  serviceGet: 'service_get',
  serviceCreate: 'service_create',
  serviceUpdate: 'service_update',
  serviceDelete: 'service_delete',
} as const;

export type CmdName = (typeof CMD)[keyof typeof CMD];
