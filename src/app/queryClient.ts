import { MutationCache, QueryClient } from '@tanstack/react-query';
import { IpcError } from '@/shared/ipc/invoke';
import { pushToast } from './uiSlice';
import { store } from './store';

/** Every failed mutation shows its plain-language message once; a declined confirm is silent. */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 2_000, refetchOnWindowFocus: true, retry: 1 },
  },
  mutationCache: new MutationCache({
    onError(error) {
      if (error instanceof IpcError && error.kind === 'cancelled') return;
      store.dispatch(pushToast(error.message));
    },
  }),
});
