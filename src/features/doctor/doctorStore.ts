import { CMD } from '@/shared/ipc/commands';
import { onIpcEvent } from '@/shared/ipc/events';
import { ipcVoid } from '@/shared/ipc/invoke';
import { DoneSchema, FindingSchema, type Finding } from './api/doctor.schema';
import { upsert } from './doctorGroups';

/**
 * Doctor's results live outside the page, so leaving and coming back neither re-reads the disk nor
 * starts a second scan, and events that arrive while the page is closed are not lost.
 */
export type DoctorState = { findings: Finding[]; scanning: boolean; millis: number | null };

const empty: DoctorState = { findings: [], scanning: false, millis: null };
let state = empty;
const subs = new Set<() => void>();
const set = (s: DoctorState) => {
  state = s;
  subs.forEach((f) => f());
};

export const getState = () => state;
export const subscribe = (f: () => void) => {
  subs.add(f);
  return () => void subs.delete(f);
};

/** Starts a fresh result set; false if a scan is already running. */
export function begin(): boolean {
  if (state.scanning) return false;
  set({ findings: [], scanning: true, millis: null });
  return true;
}
export const addFinding = (f: Finding) => set({ ...state, findings: upsert(state.findings, f) });
export const finish = (millis: number) => set({ ...state, scanning: false, millis });
export const failScan = () => set({ ...state, scanning: false });
export const resetForTest = () => set(empty);

let listening = false;
function listen() {
  if (listening) return;
  listening = true;
  void onIpcEvent('doctor://finding', FindingSchema, addFinding);
  void onIpcEvent('doctor://done', DoneSchema, (d) => finish(d.millis));
}

/** Runs a scan unless one is already running. */
export function scan(report: (message: string) => void) {
  listen();
  if (!begin()) return;
  ipcVoid(CMD.doctorScan).catch((e: Error) => {
    failScan();
    report(e.message);
  });
}
