import { useState } from 'react';
import { useAppDispatch } from '@/app/store';
import { showLogs } from '@/app/uiSlice';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
import { useDeleteService, useSaveService, useServices } from '../../api/useServices';
import { useRuntime } from '../../api/useRuntime';
import type { ServiceSpec } from '../../api/services.schema';
import { ServiceForm } from '../../components/ServiceForm/ServiceForm';
import { ServiceRow } from '../../components/ServiceRow/ServiceRow';

export function ServicesPage() {
  const dispatch = useAppDispatch();
  const services = useServices();
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
      <div className="flex items-center">
        <h1 className="text-3xl font-extrabold">Services</h1>
        <Button variant="primary" className="ml-auto" onClick={() => setEditing(null)}>
          Add service
        </Button>
      </div>
      {services.isError && (
        <Card role="alert" className="text-danger">
          Could not load services: {services.error.message}
        </Card>
      )}
      {services.data?.length === 0 && (
        <Card>
          <p className="font-medium">No services yet.</p>
          <p className="text-sm text-fg-muted">
            Add a command you run during development, like a dev server or a database, and Orbit
            will start, stop and watch it for you.
          </p>
        </Card>
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
    </div>
  );
}
