import { QueryClientProvider } from '@tanstack/react-query';
import { Provider } from 'react-redux';
import { queryClient } from './queryClient';
import { Bootstrap } from './Bootstrap';
import { store, useAppSelector } from './store';
import { ProjectsPage } from '@/features/projects/pages/ProjectsPage';
import { DoctorPage } from '@/features/doctor/pages/DoctorPage';
import { SettingsPage } from '@/features/settings/pages/SettingsPage';
import { Palette } from '@/shared/palette/Palette';
import { LogsPage } from '@/features/logs/pages/LogsPage';
import { ServicesPage } from '@/features/services/pages/ServicesPage/ServicesPage';
import { AppShell } from '@/shared/layout/AppShell';
import { ToastHost } from '@/shared/ui/Toast';

function Content() {
  const module = useAppSelector((s) => s.ui.activeModule);
  return (
    <AppShell>
      {module === 'services' && <ServicesPage />}
      {module === 'projects' && <ProjectsPage />}
      {module === 'doctor' && <DoctorPage />}
      {module === 'logs' && <LogsPage />}
      {module === 'settings' && <SettingsPage />}
      <Palette />
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
