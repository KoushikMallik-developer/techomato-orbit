import { ArrowRight } from 'lucide-react';
import { Button } from '@/shared/ui/Button';
import { Pill } from '@/shared/ui/Pill';
import { tableWrap, td, th, tr } from '@/shared/ui/table';
import type { Package } from '../api/packages.schema';
import type { View } from '../api/usePackages';
import { isManageable } from '../packageFilter';

type Props = {
  rows: Package[];
  view: View;
  busy: boolean;
  onInstall: (p: Package) => void;
  onUpgrade: (p: Package) => void;
  onUninstall: (p: Package) => void;
};

export function PackageTable({ rows, view, busy, onInstall, onUpgrade, onUninstall }: Props) {
  return (
    <div className={tableWrap}>
      <table className="w-full border-collapse">
        <thead>
          <tr>
            {['Package', 'Version', 'Source', ''].map((h) => (
              <th key={h} className={th}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((p) => {
            const ok = isManageable(p.id);
            const why = ok
              ? undefined
              : 'Orbit cannot manage this entry (its id is not a package id)';
            return (
              <tr key={`${p.id}|${p.source ?? ''}`} className={tr}>
                <td className={`${td} max-w-[22rem]`}>
                  <div className="truncate font-medium" title={p.name}>
                    {p.name}
                  </div>
                  <div className="truncate font-mono text-xs text-fg-subtle" title={p.id}>
                    {p.id}
                  </div>
                </td>
                <td className={`${td} whitespace-nowrap font-mono`}>
                  <span className="text-fg-muted">{p.version}</span>
                  {p.available && (
                    <span className="ml-2 inline-flex items-center gap-1.5">
                      <ArrowRight aria-hidden className="size-3.5 text-fg-subtle" />
                      <Pill tone="warn">{p.available}</Pill>
                    </span>
                  )}
                </td>
                <td className={`${td} text-fg-muted`}>{p.source ?? '—'}</td>
                <td className={`${td} whitespace-nowrap text-right`}>
                  {view === 'search' ? (
                    <Button
                      size="sm"
                      variant="primary"
                      disabled={busy || !ok}
                      title={why}
                      onClick={() => onInstall(p)}
                    >
                      Install
                    </Button>
                  ) : (
                    <>
                      {p.available && (
                        <Button
                          size="sm"
                          variant="primary"
                          disabled={busy || !ok}
                          title={why}
                          onClick={() => onUpgrade(p)}
                        >
                          Upgrade
                        </Button>
                      )}{' '}
                      {view === 'installed' && (
                        <Button
                          size="sm"
                          variant="danger"
                          disabled={busy || !ok}
                          title={why}
                          onClick={() => onUninstall(p)}
                        >
                          Uninstall
                        </Button>
                      )}
                    </>
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
