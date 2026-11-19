import { useState, type ReactNode } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { RefreshCw } from 'lucide-react';
import { Button } from '@/shared/ui/Button';

type Props = {
  title: string;
  summary?: ReactNode;
  actions?: ReactNode;
  /** Replaces the default refresh (refetch every query on screen), for pages with their own data. */
  onRefresh?: () => void;
  refreshing?: boolean;
};

/** The title row every page starts with: name, one line of live status, actions and Refresh on the right. */
export function PageHeader({ title, summary, actions, onRefresh, refreshing }: Props) {
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const spinning = refreshing ?? busy;
  const refresh =
    onRefresh ??
    (() => {
      setBusy(true);
      void qc.invalidateQueries().finally(() => setBusy(false));
    });
  return (
    <header className="flex flex-wrap items-end gap-x-4 gap-y-2">
      <div className="min-w-0">
        <h1 className="text-[1.75rem] leading-tight font-extrabold">{title}</h1>
        {summary && <p className="mt-0.5 text-sm text-fg-muted">{summary}</p>}
      </div>
      <div className="ml-auto flex items-center gap-2">
        {actions}
        <Button aria-label="Refresh data" disabled={spinning} onClick={refresh}>
          <RefreshCw aria-hidden className={`size-4 ${spinning ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>
    </header>
  );
}
