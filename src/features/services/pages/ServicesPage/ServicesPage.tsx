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
import { usePorts } from '@/features/ports/api/usePorts';
import { listeningPorts } from '../../listeningPorts';
import { AddDatabase } from '@/features/databases/components/AddDatabase';
import { ServiceRow } from '../../components/ServiceRow/ServiceRow';
import { DetectedServices } from '../../components/DetectedServices';
import { useDetected } from '../../api/useDetected';

export function ServicesPage() {
  const dispatch = useAppDispatch();
  const services = useServices();
  const runtime = useRuntime();
  // Only look while something is running: each poll lists every port and process on the PC.
  const anyRunning = Object.values(runtime).some((r) => r.pid != null);
  const ports = usePorts(anyRunning).data;
  const detected = useDetected();
  const save = useSaveService();
  const del = useDeleteService();
  // `undefined` = list, `null` = adding, a spec = editing.
  const [editing, setEditing] = useState<ServiceSpec | null | undefined>(undefined);
  const [addingDb, setAddingDb] = useState(false);

  if (addingDb) {
    return <AddDatabase onDone={() => setAddingDb(false)} onCancel={() => setAddingDb(false)} />;
  }

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
          <>
            <Button onClick={() => setAddingDb(true)}>Add database</Button>
            <Button variant="primary" onClick={() => setEditing(null)}>
              Add service
            </Button>
          </>
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
          hint="Add a database in one click, or any command you run during development, and Orbit will start, stop and watch it for you."
          action={
            <>
              <Button variant="primary" onClick={() => setAddingDb(true)}>
                Add database
              </Button>
              <Button onClick={() => setEditing(null)}>Add service</Button>
            </>
          }
        />
      )}
      <DetectedServices skipped={detected.skipped} />
      <div className="grid gap-4 md:grid-cols-2 2xl:grid-cols-3">
        {services.data?.map((s) => (
          <ServiceRow
            key={s.id}
            spec={s}
            runtime={runtime[s.id]}
            livePorts={runtime[s.id]?.pid != null ? listeningPorts(ports, s.id, s.port) : []}
            onEdit={() => setEditing(s)}
            onDelete={() => del.mutate(s.id)}
            onLogs={() => dispatch(showLogs(s.id))}
          />
        ))}
      </div>
    </div>
  );
}
