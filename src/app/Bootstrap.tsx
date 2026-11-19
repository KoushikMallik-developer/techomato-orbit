import { useEffect, type ReactNode } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { z } from 'zod';
import { CMD } from '@/shared/ipc/commands';
import { ipc } from '@/shared/ipc/invoke';
import { useAppSelector } from './store';

const inTauri = () => '__TAURI_INTERNALS__' in window;

/** Applies the theme, then shows the window only after fonts and first paint (no white flash). */
export function Bootstrap({ children }: { children: ReactNode }) {
  const theme = useAppSelector((s) => s.ui.theme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    if (!inTauri()) return;
    let cancelled = false;
    // Started by Windows at sign-in: stay in the tray; the tray's Open shows the window.
    const background = ipc(CMD.launchedInBackground, {}, z.boolean()).catch(() => false);
    void Promise.all([document.fonts.ready, background]).then(([, hidden]) =>
      requestAnimationFrame(() => {
        if (!cancelled && !hidden) void getCurrentWindow().show();
      }),
    );
    return () => {
      cancelled = true;
    };
  }, []);

  return <>{children}</>;
}
