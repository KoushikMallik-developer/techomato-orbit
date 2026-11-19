import { Button } from '@/shared/ui/Button';
import { Pill } from '@/shared/ui/Pill';
import { tableWrap, td, th, tr } from '@/shared/ui/table';
import type { PortRow } from '../api/ports.schema';
import { canEnd } from '../portFilter';

type Props = {
  rows: PortRow[];
  watch: number[];
  busy: boolean;
  onEnd: (row: PortRow) => void;
  onToggleWatch: (port: number) => void;
};

export function PortTable({ rows, watch, busy, onEnd, onToggleWatch }: Props) {
  return (
    <div className={tableWrap}>
      <table className="w-full border-collapse">
        <thead>
          <tr>
            {['Port', 'Address', 'Process', 'PID', 'Service', ''].map((h) => (
              <th key={h} className={th}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={`${r.proto}-${r.localAddr}-${r.port}-${r.pid}`} className={tr}>
              <td className={`${td} whitespace-nowrap`}>
                <span className="mr-2 font-mono font-semibold">{r.port}</span>
                <Pill>{r.proto.toUpperCase()}</Pill>
              </td>
              <td
                className={`${td} max-w-[11rem] truncate font-mono text-fg-muted`}
                title={r.localAddr}
              >
                {r.localAddr}
              </td>
              <td className={`${td} max-w-[14rem]`}>
                <div className="truncate font-medium">
                  {r.processName ?? <span className="text-fg-muted">Unknown</span>}
                </div>
                <div className="truncate text-xs text-fg-subtle" title={r.path ?? undefined}>
                  {r.path ?? 'Path not readable'}
                </div>
              </td>
              <td className={`${td} font-mono text-fg-muted`}>{r.pid}</td>
              <td className={td}>
                {r.serviceName ? (
                  <Pill tone="running" dot>
                    {r.serviceName}
                  </Pill>
                ) : r.conflictsWith ? (
                  <Pill tone="danger" dot>
                    Blocks {r.conflictsWith}
                  </Pill>
                ) : (
                  <span className="text-fg-subtle">—</span>
                )}
              </td>
              <td className={`${td} whitespace-nowrap text-right`}>
                <Button size="sm" onClick={() => onToggleWatch(r.port)}>
                  {watch.includes(r.port) ? 'Unwatch' : 'Watch'}
                </Button>{' '}
                {canEnd(r) && (
                  <Button
                    size="sm"
                    variant="danger"
                    disabled={busy || r.createdAtMs === null}
                    title={
                      r.createdAtMs === null
                        ? 'Windows does not let Orbit inspect this process'
                        : undefined
                    }
                    onClick={() => onEnd(r)}
                  >
                    End
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
