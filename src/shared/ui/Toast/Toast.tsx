import { useEffect } from 'react';
import { dismissToast } from '@/app/uiSlice';
import { useAppDispatch, useAppSelector } from '@/app/store';

function ToastItem({ id, text }: { id: number; text: string }) {
  const dispatch = useAppDispatch();
  useEffect(() => {
    const t = setTimeout(() => dispatch(dismissToast(id)), 8000);
    return () => clearTimeout(t);
  }, [dispatch, id]);
  return (
    <div
      role="alert"
      className="flex max-w-md items-start gap-3 rounded-card border border-danger bg-paper px-4 py-3 text-sm shadow-lg"
    >
      <span className="min-w-0 flex-1 break-words">{text}</span>
      <button
        type="button"
        aria-label="Dismiss"
        onClick={() => dispatch(dismissToast(id))}
        className="font-semibold text-fg-muted hover:text-fg"
      >
        ×
      </button>
    </div>
  );
}

export function ToastHost() {
  const toasts = useAppSelector((s) => s.ui.toasts);
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-50 flex flex-col gap-2 *:pointer-events-auto">
      {toasts.map((t) => (
        <ToastItem key={t.id} {...t} />
      ))}
    </div>
  );
}
