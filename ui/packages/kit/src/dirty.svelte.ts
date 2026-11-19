// Unsaved-changes protection. Call from a component's <script>:
//   const isDirty = $derived(JSON.stringify(form) !== saved);
//   useUnsavedGuard(() => isDirty);

import { addNavigationGuard, markDirty } from "./router.svelte";
import { confirmDialog } from "./dialogs.svelte";

export function useUnsavedGuard(isDirty: () => boolean, message = "You have unsaved changes. Leave without saving?") {
  $effect(() => {
    const remove = addNavigationGuard(async () => {
      if (!isDirty()) return true;
      return confirmDialog({ title: "Discard changes?", message, confirmLabel: "Discard", danger: true });
    });
    return remove;
  });
  $effect(() => {
    const d = isDirty();
    if (!d) return;
    markDirty(true);
    return () => markDirty(false);
  });
}

/** Snapshot helper for dirty tracking of plain JSON-like state. */
export function snapshot<T>(v: T): string {
  return JSON.stringify(v);
}
