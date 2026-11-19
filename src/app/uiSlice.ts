import { createSlice, type PayloadAction } from '@reduxjs/toolkit';

export type ModuleKey = 'services' | 'logs' | 'settings';
export type ThemePref = 'light' | 'dark';

type UiState = { activeModule: ModuleKey; theme: ThemePref };

const THEME_KEY = 'orbit.theme';

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
  initialState: (): UiState => ({ activeModule: 'services', theme: initialTheme() }),
  reducers: {
    setModule(state, a: PayloadAction<ModuleKey>) {
      state.activeModule = a.payload;
    },
    setTheme(state, a: PayloadAction<ThemePref>) {
      state.theme = a.payload;
      try {
        localStorage.setItem(THEME_KEY, a.payload);
      } catch {
        /* a failed save only loses the preference for next launch */
      }
    },
  },
});

export const { setModule, setTheme } = uiSlice.actions;
