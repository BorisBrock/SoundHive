/** Themes that the CSS in app.css understands. */
export const THEMES = [
  { id: "midnight", label: "Midnight", dark: true },
  { id: "nord", label: "Nord", dark: true },
  { id: "dracula", label: "Dracula", dark: true },
  { id: "gruvbox", label: "Gruvbox", dark: true },
  { id: "catppuccin-mocha", label: "Catppuccin Mocha", dark: true },
  { id: "tokyo-night", label: "Tokyo Night", dark: true },
  { id: "solarized-dark", label: "Solarized Dark", dark: true },
  { id: "high-contrast", label: "High Contrast", dark: true },
  { id: "daylight", label: "Daylight", dark: false },
  { id: "solarized-light", label: "Solarized Light", dark: false },
  { id: "catppuccin-latte", label: "Catppuccin Latte", dark: false },
  { id: "paper", label: "Paper", dark: false },
] as const;

export function applyTheme(theme: string) {
  document.documentElement.dataset.theme = theme;
}
