export type Theme = 'system' | 'light' | 'dark';

/** Apply the persisted theme to the document without touching web storage. */
export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme === 'system' ? 'light dark' : theme;
}
