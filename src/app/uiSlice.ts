import { createSlice, type PayloadAction } from '@reduxjs/toolkit';

export type ModuleKey = 'services' | 'projects' | 'ports' | 'logs' | 'settings';
export type ThemePref = 'light' | 'dark';
export type Toast = { id: number; text: string };

type UiState = {
  activeModule: ModuleKey;
  theme: ThemePref;
  /** Service whose logs the Logs page shows. */
  logServiceId: string | null;
  toasts: Toast[];
};

const THEME_KEY = 'orbit.theme';
let nextToastId = 1;

function initialTheme(): ThemePref {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === 'light' || saved === 'dark') return saved;
  } catch {
    /* storage can be unavailable; fall through to the system preference */
  }
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export const uiSlice = createSlice({
  name: 'ui',
  initialState: (): UiState => ({
    activeModule: 'services',
    theme: initialTheme(),
    logServiceId: null,
    toasts: [],
  }),
  reducers: {
    setModule(state, a: PayloadAction<ModuleKey>) {
      state.activeModule = a.payload;
    },
    showLogs(state, a: PayloadAction<string>) {
      state.logServiceId = a.payload;
      state.activeModule = 'logs';
    },
    setTheme(state, a: PayloadAction<ThemePref>) {
      state.theme = a.payload;
      try {
        localStorage.setItem(THEME_KEY, a.payload);
      } catch {
        /* a failed save only loses the preference for next launch */
      }
    },
    pushToast: {
      reducer(state, a: PayloadAction<Toast>) {
        state.toasts = [...state.toasts.slice(-3), a.payload];
      },
      prepare: (text: string) => ({ payload: { id: nextToastId++, text } }),
    },
    dismissToast(state, a: PayloadAction<number>) {
      state.toasts = state.toasts.filter((t) => t.id !== a.payload);
    },
  },
});

export const { setModule, showLogs, setTheme, pushToast, dismissToast } = uiSlice.actions;
