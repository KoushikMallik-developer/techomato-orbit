import type { ReactNode } from 'react';
import { Sidebar } from '@/shared/layout/Sidebar';

export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full">
      <Sidebar />
      <main className="min-w-0 flex-1 overflow-auto p-8">{children}</main>
    </div>
  );
}
