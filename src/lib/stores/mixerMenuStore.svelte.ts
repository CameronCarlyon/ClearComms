// Keep only one expandable mixer menu open at a time.

const openMenu = $state<{ id: string | null }>({ id: null });
let nextMenuId = 0;

export function registerMixerMenu(isOpen: () => boolean, close: () => void): void {
  const menuId = `mixer-menu-${nextMenuId++}`;

  $effect(() => {
    if (!isOpen()) return;
    openMenu.id = menuId;
    // Release on close/unmount: otherwise a stale id locks every menu shut.
    return () => {
      if (openMenu.id === menuId) openMenu.id = null;
    };
  });

  $effect(() => {
    // Only reacts to openMenu.id, never isOpen(): reading our own state here
    // would race the claim above and nothing could open.
    if (openMenu.id !== null && openMenu.id !== menuId) close();
  });
}
