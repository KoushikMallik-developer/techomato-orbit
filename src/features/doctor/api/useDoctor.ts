import { useEffect, useSyncExternalStore } from 'react';
import { useMutation } from '@tanstack/react-query';
import { store } from '@/app/store';
import { pushToast } from '@/app/uiSlice';
import { CMD } from '@/shared/ipc/commands';
import { ipc } from '@/shared/ipc/invoke';
import { getState, scan, subscribe } from '../doctorStore';
import { FixResultSchema, type Fix } from './doctor.schema';

const report = (m: string) => store.dispatch(pushToast(m));
const rescan = () => scan(report);

/** Scans the first time the page is opened; later visits show the last result until "Scan again". */
export function useDoctor() {
  const s = useSyncExternalStore(subscribe, getState);
  useEffect(() => {
    const cur = getState();
    if (!cur.scanning && cur.millis === null && cur.findings.length === 0) rescan();
  }, []);

  const fix = useMutation({
    mutationFn: (f: Fix) => ipc(CMD.doctorFix, { fix: f }, FixResultSchema),
    onSuccess: (r) => {
      report(r.message);
      rescan();
    },
  });

  return { ...s, scan: rescan, fix };
}
