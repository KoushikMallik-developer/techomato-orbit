import { Button } from '@/shared/ui/Button';
import { Pill } from '@/shared/ui/Pill';
import type { WinService } from '../api/winservices.schema';
import type { WinControl } from '../api/useWinServices';
import { winActions } from '../winFilter';

type Props = {
  rows: WinService[];
  pins: string[];
  busy: boolean;
  onControl: (name: string, action: WinControl) => void;
  onTogglePin: (name: string) => void;
};

const th = 'px-3 py-2 text-left text-xs font-semibold uppercase tracking-wide text-fg-muted';
const td = 'px-3 py-2 align-top text-sm';
const START: Record<WinService['startType'], string> = {
  auto: 'Automatic',
  autoDelayed: 'Automatic (delayed)',
  manual: 'Manual',
  disabled: 'Disabled',
  boot: 'Boot',
  system: 'System',
};
const STATE: Record<WinService['state'], string> = {
  stopped: 'Stopped',
  startPending: 'Starting',
  stopPending: 'Stopping',
  running: 'Running',
  continuePending: 'Resuming',
  pausePending: 'Pausing',
  paused: 'Paused',
};

export function WinServiceTable({ rows, pins, busy, onControl, onTogglePin }: Props) {
  const pinned = (n: string) => pins.some((p) => p.toLowerCase() === n.toLowerCase());
  return (
    <div className="overflow-x-auto rounded-card border border-border bg-paper">
      <table className="w-full min-w-[48rem] border-collapse">
        <thead>
          <tr className="border-b border-border">
            {['Service', 'State', 'Start type', 'PID', ''].map((h) => (
              <th key={h} className={th}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((s) => {
            const can = winActions(s.state, s.protected);
            return (
              <tr key={s.name} className="border-b border-border last:border-0">
                <td className={td}>
                  <div className="font-medium">
                    {s.displayName}
                    {s.protected && (
                      <span className="ml-2" title="Part of Windows; Orbit does not manage it">
                        <Pill>🔒 Protected</Pill>
                      </span>
                    )}
                  </div>
                  <div className="font-mono text-xs text-fg-muted">{s.name}</div>
                </td>
                <td className={td}>
                  <Pill on={s.state === 'running'}>{STATE[s.state]}</Pill>
                </td>
                <td className={td}>{START[s.startType]}</td>
                <td className={`${td} font-mono`}>{s.pid ?? '—'}</td>
                <td className={`${td} whitespace-nowrap text-right`}>
                  {!s.protected && (
                    <Button size="sm" onClick={() => onTogglePin(s.name)}>
                      {pinned(s.name) ? 'Unpin' : 'Pin'}
                    </Button>
                  )}{' '}
                  {can.start && (
                    <Button size="sm" disabled={busy} onClick={() => onControl(s.name, 'start')}>
                      Start
                    </Button>
                  )}{' '}
                  {can.stop && (
                    <Button size="sm" disabled={busy} onClick={() => onControl(s.name, 'stop')}>
                      Stop
                    </Button>
                  )}{' '}
                  {can.restart && (
                    <Button size="sm" disabled={busy} onClick={() => onControl(s.name, 'restart')}>
                      Restart
                    </Button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
