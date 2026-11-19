/** Every Tauri command name. No raw command strings anywhere else. */
export const CMD = {
  serviceList: 'service_list',
} as const;

export type CmdName = (typeof CMD)[keyof typeof CMD];
