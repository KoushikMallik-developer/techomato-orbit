import type { ReactNode } from 'react';

type Props = { title: string; summary?: ReactNode; actions?: ReactNode };

/** The title row every page starts with: name, one line of live status, actions on the right. */
export function PageHeader({ title, summary, actions }: Props) {
  return (
    <header className="flex flex-wrap items-end gap-x-4 gap-y-2">
      <div className="min-w-0">
        <h1 className="text-[1.75rem] leading-tight font-extrabold">{title}</h1>
        {summary && <p className="mt-0.5 text-sm text-fg-muted">{summary}</p>}
      </div>
      {actions && <div className="ml-auto flex items-center gap-2">{actions}</div>}
    </header>
  );
}
