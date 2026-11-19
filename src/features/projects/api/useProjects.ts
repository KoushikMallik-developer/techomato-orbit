import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { useAppDispatch } from '@/app/store';
import { pushToast } from '@/app/uiSlice';
import { CMD } from '@/shared/ipc/commands';
import { useIpcEvent } from '@/shared/ipc/events';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';
import { servicesKey } from '@/features/services/api/useServices';
import {
  EdgeSchema,
  ProgressSchema,
  ProjectSchema,
  RunReportSchema,
  reportProblems,
  type Edge,
  type Project,
} from './projects.schema';

export const projectsKey = ['projects'] as const;
export const edgesKey = ['edges'] as const;

export function useProjects() {
  return useQuery({
    queryKey: projectsKey,
    queryFn: () => ipc(CMD.projectList, {}, z.array(ProjectSchema)),
  });
}

export function useEdges() {
  return useQuery({
    queryKey: edgesKey,
    queryFn: () => ipc(CMD.dependencyEdges, {}, z.array(EdgeSchema)),
  });
}

export function useSaveProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (project: Project) =>
      ipc(project.id ? CMD.projectUpdate : CMD.projectCreate, { project }, ProjectSchema),
    onSuccess: () => qc.invalidateQueries({ queryKey: projectsKey }),
  });
}

/** Archive, delete (native confirm) and duplicate. */
export function useProjectActions() {
  const qc = useQueryClient();
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: projectsKey });
    void qc.invalidateQueries({ queryKey: servicesKey });
    void qc.invalidateQueries({ queryKey: edgesKey });
  };
  return {
    archive: useMutation({
      mutationFn: (v: { id: string; archived: boolean }) => ipcVoid(CMD.projectArchive, v),
      onSuccess: refresh,
    }),
    remove: useMutation({
      mutationFn: (id: string) => ipcVoid(CMD.projectDelete, { id }),
      onSuccess: refresh,
    }),
    duplicate: useMutation({
      mutationFn: (id: string) => ipc(CMD.projectDuplicate, { id }, ProjectSchema),
      onSuccess: refresh,
    }),
  };
}

export function useSetDependencies() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { serviceId: string; deps: Pick<Edge, 'dependsOnId' | 'onFailure'>[] }) =>
      ipcVoid(CMD.dependencySet, v),
    // A rejected loop shows as a toast (global handler); the list is refreshed either way.
    onSettled: () => qc.invalidateQueries({ queryKey: edgesKey }),
  });
}

/** Start / stop / restart a whole project; problems in the run are shown as one message. */
export function useRunProject() {
  const dispatch = useAppDispatch();
  const run = (cmd: typeof CMD.projectStart | typeof CMD.projectStop | typeof CMD.projectRestart) =>
    useMutation({
      mutationFn: (v: { id: string; selection?: string[] | null }) =>
        ipc(
          cmd,
          { id: v.id, ...(cmd === CMD.projectStart ? { selection: v.selection ?? null } : {}) },
          RunReportSchema,
        ),
      onSuccess: (report) => {
        const problems = reportProblems(report);
        if (problems) dispatch(pushToast(problems));
      },
    });
  return {
    start: run(CMD.projectStart),
    stop: run(CMD.projectStop),
    restart: run(CMD.projectRestart),
  };
}

/** Latest step per service while a project run is in flight (`starting`, `ready`, ...). */
export function useProjectProgress(): Record<string, string> {
  const [steps, setSteps] = useState<Record<string, string>>({});
  useIpcEvent('project://progress', ProgressSchema, (p) =>
    setSteps((old) => ({ ...old, [p.serviceId]: p.step })),
  );
  return steps;
}
