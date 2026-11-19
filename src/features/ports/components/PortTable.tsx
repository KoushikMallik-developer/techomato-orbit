import { Button } from '@/shared/ui/Button';
import type { PortRow } from '../api/ports.schema';
import { canEnd } from '../portFilter';

type Props = {
  rows: PortRow[];
  watch: number[];
  busy: boolean;
  onEnd: (row: PortRow) => void;
  onToggleWatch: (port: number) => void;
};

const th = 'px-3 py-2 text-left text-xs font-semibold uppercase tracking-wide text-fg-muted';
const td = 'px-3 py-2 align-top text-sm';

export function PortTable({ rows, watch, busy, onEnd, onToggleWatch }: Props) {
  return (
    <div className="overflow-x-auto rounded-card border border-border bg-paper">
      <table className="w-full min-w-[56rem] border-collapse">
        <thead>
          <tr className="border-b border-border">
            {['Proto', 'Address', 'Port', 'Process', 'PID', 'Path', 'Service', ''].map((h) => (
              <th key={h} className={th}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr
              key={`${r.proto}-${r.localAddr}-${r.port}-${r.pid}`}
              className="border-b border-border last:border-0"
            >
              <td className={td}>{r.proto.toUpperCase()}</td>
              <td className={`${td} font-mono`}>{r.localAddr}</td>
              <td className={`${td} font-mono font-semibold`}>{r.port}</td>
              <td className={td}>
                {r.processName ?? <span className="text-fg-muted">Unknown</span>}
              </td>
              <td className={`${td} font-mono`}>{r.pid}</td>
              <td className={`${td} max-w-xs break-all text-fg-muted`} title={r.path ?? undefined}>
                {r.path ?? 'Not readable'}
              </td>
              <td className={td}>
                {r.serviceName ??
                  (r.conflictsWith ? (
                    <span className="text-danger">Blocks {r.conflictsWith}</span>
                  ) : (
                    <span className="text-fg-muted">—</span>
                  ))}
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
