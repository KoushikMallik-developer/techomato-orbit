import { QueryClientProvider } from '@tanstack/react-query';
import { Provider } from 'react-redux';
import { queryClient } from './queryClient';
import { Bootstrap } from './Bootstrap';
import { store, useAppSelector } from './store';
import { PortsPage } from '@/features/ports/pages/PortsPage';
import { LogsPage } from '@/features/logs/pages/LogsPage';
import { ServicesPage } from '@/features/services/pages/ServicesPage/ServicesPage';
import { AppShell } from '@/shared/layout/AppShell';
import { Card } from '@/shared/ui/Card';
import { ToastHost } from '@/shared/ui/Toast';

function Content() {
  const module = useAppSelector((s) => s.ui.activeModule);
  return (
    <AppShell>
      {module === 'services' && <ServicesPage />}
      {module === 'ports' && <PortsPage />}
      {module === 'logs' && <LogsPage />}
      {module === 'settings' && (
        <>
          <h1 className="mb-4 text-3xl font-extrabold">Settings</h1>
          <Card>Nothing here yet.</Card>
        </>
      )}
      <ToastHost />
    </AppShell>
  );
}

export function App() {
  return (
    <Provider store={store}>
      <QueryClientProvider client={queryClient}>
        <Bootstrap>
          <Content />
        </Bootstrap>
      </QueryClientProvider>
    </Provider>
  );
}
