import { Fragment, useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { FolderKanban } from 'lucide-react';
import { Card } from '@/shared/ui/Card';
import { EmptyState } from '@/shared/ui/EmptyState';
import { PageHeader } from '@/shared/ui/PageHeader';
import { useSaveService, useServices } from '@/features/services/api/useServices';
import { useRuntime, useServiceActions } from '@/features/services/api/useRuntime';
import type { Project } from '../api/projects.schema';
import {
  useEdges,
  useProjectActions,
  useProjectProgress,
  useProjects,
  useRunProject,
  useSaveProject,
  useSetDependencies,
} from '../api/useProjects';
import { ProjectCard } from '../components/ProjectCard';
import { ProjectEditor } from '../components/ProjectEditor';

const countLabel = (n: number) => `${n} ${n === 1 ? 'project' : 'projects'}`;

export function ProjectsPage() {
  const projects = useProjects();
  const services = useServices();
  const edges = useEdges();
  const runtimes = useRuntime();
  const progress = useProjectProgress();
  const run = useRunProject();
  const actions = useProjectActions();
  const saveProject = useSaveProject();
  const saveService = useSaveService();
  const setDeps = useSetDependencies();
  const serviceActions = useServiceActions();
  // `undefined` = list, `null` = creating, a project = editing.
  const [editing, setEditing] = useState<Project | null | undefined>(undefined);
  const busy = run.start.isPending || run.stop.isPending || run.restart.isPending;

  if (editing !== undefined) {
    const current = editing && projects.data?.find((p) => p.id === editing.id);
    return (
      <ProjectEditor
        initial={current ?? editing}
        services={services.data ?? []}
        edges={edges.data ?? []}
        saving={saveProject.isPending || saveService.isPending}
        onCancel={() => setEditing(undefined)}
        onSetDeps={(serviceId, deps) => setDeps.mutate({ serviceId, deps })}
        onSave={async (project, memberIds) => {
          try {
            const saved = await saveProject.mutateAsync(project);
            // Move services into / out of this project.
            for (const s of services.data ?? []) {
              const wants = memberIds.includes(s.id);
              const has = s.projectId === saved.id;
              if (wants !== has && (wants || has)) {
                await saveService.mutateAsync({ ...s, projectId: wants ? saved.id : null });
              }
            }
            setEditing(undefined);
          } catch {
            /* the global handler already showed the message */
          }
        }}
      />
    );
  }

  const all = [...(projects.data ?? [])].sort((a, b) => Number(a.archived) - Number(b.archived));
  const firstArchived = all.findIndex((p) => p.archived);
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Projects"
        summary={
          projects.data
            ? countLabel(projects.data.filter((p) => !p.archived).length)
            : 'Loading projects'
        }
        actions={
          <Button variant="primary" onClick={() => setEditing(null)}>
            New project
          </Button>
        }
      />
      {projects.isError && (
        <Card role="alert" className="text-danger">
          Could not load projects: {projects.error.message}
        </Card>
      )}
      {projects.data?.length === 0 && (
        <EmptyState
          icon={FolderKanban}
          title="No projects yet"
          hint="A project groups the services of one application, like a database, an API and a web app, so you can start them together in the right order."
        />
      )}
      {all.map((p, i) => (
        <Fragment key={p.id}>
          {i === firstArchived && (
            <h2 className="mt-2 text-sm font-semibold text-fg-muted">Archived</h2>
          )}
          <ProjectCard
            project={p}
            members={(services.data ?? []).filter((s) => s.projectId === p.id)}
            runtimes={runtimes}
            progress={progress}
            busy={busy}
            onStart={(selection) => run.start.mutate({ id: p.id, selection })}
            onStop={() => run.stop.mutate({ id: p.id })}
            onRestart={() => run.restart.mutate({ id: p.id })}
            onStopSelected={(ids) => ids.forEach((id) => serviceActions.stop.mutate(id))}
            onEdit={() => setEditing(p)}
            onDuplicate={() => actions.duplicate.mutate(p.id)}
            onArchive={() => actions.archive.mutate({ id: p.id, archived: !p.archived })}
            onDelete={() => actions.remove.mutate(p.id)}
          />
        </Fragment>
      ))}
    </div>
  );
}
