import type { IconName } from "./components/Icon.svelte";

export interface MenuItem {
  label: string;
  icon?: IconName;
  /** Keyboard shortcut, shown on the right. */
  hint?: string;
  action: () => void;
  danger?: boolean;
}

/** Right-click menu; `null` entries become separators. */
class ContextMenu {
  current = $state<{ x: number; y: number; items: (MenuItem | null)[] } | null>(null);

  open(e: MouseEvent, items: (MenuItem | null)[]) {
    e.preventDefault();
    this.current = { x: e.clientX, y: e.clientY, items };
  }

  close() {
    this.current = null;
  }
}

export const contextMenu = new ContextMenu();
