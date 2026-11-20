import { CircleAlert, Info, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { dismissToast } from '@/app/uiSlice';
import { useAppDispatch, useAppSelector } from '@/app/store';

function ToastItem({ id, text, tone }: { id: number; text: string; tone: 'error' | 'info' }) {
  const error = tone === 'error';
  const dispatch = useAppDispatch();
  // Hovering or focusing a toast pauses its timer so the text can be read and copied.
  const [paused, setPaused] = useState(false);
  useEffect(() => {
    if (paused) return;
    const t = setTimeout(() => dispatch(dismissToast(id)), 8000);
    return () => clearTimeout(t);
  }, [dispatch, id, paused]);
  return (
    <div
      role={error ? 'alert' : 'status'}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
      className={`page-enter flex max-w-md items-start gap-3 rounded-card border ${error ? 'border-danger/60' : 'border-border-strong'} bg-paper px-4 py-3 text-sm shadow-pop`}
    >
      {error ? (
        <CircleAlert aria-hidden className="mt-0.5 size-4 shrink-0 text-danger" />
      ) : (
        <Info aria-hidden className="mt-0.5 size-4 shrink-0 text-fg-subtle" />
      )}
      <span className="min-w-0 flex-1 break-words">{text}</span>
      <button
        type="button"
        aria-label="Dismiss"
        onClick={() => dispatch(dismissToast(id))}
        className="font-semibold text-fg-muted hover:text-fg"
      >
        <X aria-hidden className="size-4" />
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
