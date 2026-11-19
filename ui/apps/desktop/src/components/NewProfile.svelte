<script lang="ts">
  import { Modal, Button, Field, SegmentedControl, toast, toastError } from "@axs/kit";
  import { call, type Profile } from "../api";
  import { app, refresh, switchProfile } from "../store.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();
  let name = $state("");
  let mode = $state<"copy" | "empty">("copy");
  let busy = $state(false);

  const exists = $derived(!!app.state?.profiles.some((p) => p.toLowerCase() === name.trim().toLowerCase()));
  const error = $derived(exists ? "A profile with this name exists" : null);

  $effect(() => {
    if (open) {
      name = "";
      mode = "copy";
    }
  });

  async function create() {
    const n = name.trim();
    if (!n || exists) return;
    busy = true;
    try {
      // "Empty" keeps the game executable and addon folders, not the selection.
      const base = await call<Profile>("profile_get", { name: app.state!.active_profile });
      const p: Profile = mode === "copy" ? { ...base, name: n } : { ...base, name: n, groups: [], addon_priority: [], server: null };
      await call("profile_save", { profile: p, original: null });
      await refresh();
      await switchProfile(n);
      toast(`Profile ${n} created`);
      open = false;
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="New profile" description="Profiles keep their own modset selection, game options and executable." size="sm">
  <form class="stack" onsubmit={(e) => (e.preventDefault(), create())}>
    <Field label="Name" required {error}>
      {#snippet children(id)}<input {id} bind:value={name} placeholder="e.g. Sunday Ops" aria-invalid={!!error} />{/snippet}
    </Field>
    <Field label="Start from">
      {#snippet children()}
        <SegmentedControl bind:value={mode} options={[{ value: "copy", label: `Copy of ${app.state?.active_profile}` }, { value: "empty", label: "Empty modset" }]} />
      {/snippet}
    </Field>
  </form>
  {#snippet footer()}
    <Button onclick={() => (open = false)}>Cancel</Button>
    <Button variant="primary" loading={busy} disabled={!name.trim() || exists} onclick={create}>Create profile</Button>
  {/snippet}
</Modal>
