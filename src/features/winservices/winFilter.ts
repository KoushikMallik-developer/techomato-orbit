import type { WinService } from './api/winservices.schema';

export type StateFilter = 'all' | 'running' | 'stopped';
export type StartFilter = 'all' | WinService['startType'];

/** Literal, case-insensitive match on name, display name and PID. */
export function filterWinServices(
  list: WinService[],
  query: string,
  state: StateFilter,
  startType: StartFilter,
  showAll: boolean,
): WinService[] {
  const q = query.trim().toLowerCase();
  return list.filter((s) => {
    if (!showAll && !s.developer) return false;
    if (state !== 'all' && s.state !== state) return false;
    if (startType !== 'all' && s.startType !== startType) return false;
    return (
      q === '' ||
      [s.name, s.displayName, s.pid === null ? '' : String(s.pid)].some((f) =>
        f.toLowerCase().includes(q),
      )
    );
  });
}

/** Which controls a row offers; protected and in-transition services offer none. */
export function winActions(state: WinService['state'], isProtected: boolean) {
  const none = { start: false, stop: false, restart: false };
  if (isProtected) return none;
  if (state === 'running') return { start: false, stop: true, restart: true };
  if (state === 'stopped') return { start: true, stop: false, restart: false };
  return none;
}
