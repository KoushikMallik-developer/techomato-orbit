import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { ipc, ipcVoid } from '@/shared/ipc/invoke';
import { ServiceSpecSchema, type ServiceSpec } from './services.schema';

export const servicesKey = ['services'] as const;

export function useServices() {
  return useQuery({
    queryKey: servicesKey,
    queryFn: () => ipc(CMD.serviceList, {}, z.array(ServiceSpecSchema)),
  });
}

export function useSaveService() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (spec: ServiceSpec) =>
      ipc(spec.id ? CMD.serviceUpdate : CMD.serviceCreate, { spec }, ServiceSpecSchema),
    onSuccess: () => qc.invalidateQueries({ queryKey: servicesKey }),
  });
}

export function useDeleteService() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => ipcVoid(CMD.serviceDelete, { id }),
    onSuccess: () => qc.invalidateQueries({ queryKey: servicesKey }),
  });
}
