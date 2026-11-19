import { Provider } from 'react-redux';
import { Bootstrap } from './Bootstrap';
import { store, useAppSelector } from './store';
import { AppShell } from '@/shared/layout/AppShell';
import { Card } from '@/shared/ui/Card';

function Content() {
  const module = useAppSelector((s) => s.ui.activeModule);
  return (
    <AppShell>
      <h1 className="mb-4 text-3xl font-extrabold capitalize">{module}</h1>
      <Card>Nothing here yet.</Card>
    </AppShell>
  );
}

export function App() {
  return (
    <Provider store={store}>
      <Bootstrap>
        <Content />
      </Bootstrap>
    </Provider>
  );
}
