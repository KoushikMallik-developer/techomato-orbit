import { useEffect, type ReactNode } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
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
    void document.fonts.ready.then(() =>
      requestAnimationFrame(() => {
        if (!cancelled) void getCurrentWindow().show();
      }),
    );
    return () => {
      cancelled = true;
    };
  }, []);

  return <>{children}</>;
}
