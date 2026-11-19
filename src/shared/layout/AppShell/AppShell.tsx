import type { ReactNode } from 'react';
import { useAppSelector } from '@/app/store';
import { Sidebar } from '@/shared/layout/Sidebar';

export function AppShell({ children }: { children: ReactNode }) {
  const module = useAppSelector((s) => s.ui.activeModule);
  return (
    <div className="flex h-full">
      <Sidebar />
      {/* Keyed by module so each page eases in once when you switch to it. */}
      <main key={module} className="page-enter min-w-0 flex-1 overflow-auto px-8 py-7">
        {children}
      </main>
    </div>
  );
}
