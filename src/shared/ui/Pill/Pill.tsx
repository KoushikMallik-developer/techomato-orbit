import type { ReactNode } from 'react';

export function Pill({ children, on = false }: { children: ReactNode; on?: boolean }) {
  return (
    <span
      className={`inline-flex items-center rounded-full px-3 py-0.5 text-xs font-semibold ${
        on ? 'bg-fg text-canvas' : 'bg-raised text-fg-muted'
      }`}
    >
      {children}
    </span>
  );
}
