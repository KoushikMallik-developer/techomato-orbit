import type { LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';

type Props = { icon: LucideIcon; title: string; hint?: ReactNode; action?: ReactNode };

export function EmptyState({ icon: Icon, title, hint, action }: Props) {
  return (
    <div className="flex flex-col items-center gap-3 rounded-card border border-dashed border-border-strong px-6 py-12 text-center">
      <span className="grid size-12 place-items-center rounded-full bg-accent-soft text-accent-text">
        <Icon aria-hidden className="size-6" strokeWidth={1.75} />
      </span>
      <div>
        <p className="font-display text-lg font-bold">{title}</p>
        {hint && <p className="mx-auto mt-1 max-w-md text-sm text-fg-muted">{hint}</p>}
      </div>
      {action}
    </div>
  );
}
