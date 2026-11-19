import { useState } from 'react';
import { useAppDispatch } from '@/app/store';
import { showLogs } from '@/app/uiSlice';
import { Button } from '@/shared/ui/Button';
import { ServerCog } from 'lucide-react';
import { Card } from '@/shared/ui/Card';
import { EmptyState } from '@/shared/ui/EmptyState';
import { PageHeader } from '@/shared/ui/PageHeader';
import { useDeleteService, useSaveService, useServices } from '../../api/useServices';
import { useRuntime } from '../../api/useRuntime';
import type { ServiceSpec } from '../../api/services.schema';
import { ServiceForm } from '../../components/ServiceForm/ServiceForm';
import { WindowsServicesSection } from '@/features/winservices/pages/WindowsServicesSection';
import { usePrefs } from '@/features/settings/api/usePrefs';
import { ServiceRow } from '../../components/ServiceRow/ServiceRow';

export function ServicesPage() {
  const dispatch = useAppDispatch();
  const services = useServices();
  const winEnabled = usePrefs().data?.modules.winservices !== false;
  const runtime = useRuntime();
  const save = useSaveService();
  const del = useDeleteService();
  // `undefined` = list, `null` = adding, a spec = editing.
  const [editing, setEditing] = useState<ServiceSpec | null | undefined>(undefined);

  if (editing !== undefined) {
    return (
      <ServiceForm
        initial={editing}
        saving={save.isPending}
        onCancel={() => setEditing(undefined)}
        onSave={(spec) => save.mutate(spec, { onSuccess: () => setEditing(undefined) })}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Services"
        summary={
          services.data
            ? `${Object.values(runtime).filter((r) => r.status === 'running').length} running of ${services.data.length}`
            : 'Loading services'
        }
        actions={
          <Button variant="primary" onClick={() => setEditing(null)}>
            Add service
          </Button>
        }
      />
      {services.isError && (
        <Card role="alert" className="text-danger">
          Could not load services: {services.error.message}
        </Card>
      )}
      {services.data?.length === 0 && (
        <EmptyState
          icon={ServerCog}
          title="No services yet"
          hint="Add a command you run during development, like a dev server or a database, and Orbit will start, stop and watch it for you."
          action={
            <Button variant="primary" onClick={() => setEditing(null)}>
              Add service
            </Button>
          }
        />
      )}
      {services.data?.map((s) => (
        <ServiceRow
          key={s.id}
          spec={s}
          runtime={runtime[s.id]}
          onEdit={() => setEditing(s)}
          onDelete={() => del.mutate(s.id)}
          onLogs={() => dispatch(showLogs(s.id))}
        />
      ))}
      {winEnabled && (
        <div className="mt-4">
          <WindowsServicesSection />
        </div>
      )}
    </div>
  );
}
