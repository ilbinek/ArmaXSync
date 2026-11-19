<script lang="ts">
  import { Modal, Button, Field, SegmentedControl, Alert, toast, navigate, errorText } from "@axs/kit";
  import { post } from "../api";
  import { openJob } from "../session.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let name = $state("");
  let slug = $state("");
  let slugTouched = $state(false);
  let description = $state("");
  let game = $state<"arma3" | "reforger">("arma3");
  let source = $state<"upload" | "mirror" | "external">("upload");
  let path = $state("");
  let mirror = $state("");
  let error = $state("");
  let busy = $state(false);

  $effect(() => {
    if (open) {
      name = slug = description = path = mirror = error = "";
      slugTouched = false;
      game = "arma3";
      source = "upload";
    }
  });
  const auto = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 48);
  $effect(() => {
    if (!slugTouched) slug = auto(name);
  });
  const slugErr = $derived(slug && !/^[a-z0-9][a-z0-9-]*$/.test(slug) ? "Lowercase letters, digits and dashes" : null);
  const valid = $derived(!!name.trim() && !!slug && !slugErr && (source !== "external" || !!path.trim()) && (source !== "mirror" || /^(https?|ftp):\/\//.test(mirror.trim())));

  async function create() {
    if (!valid) return;
    error = "";
    busy = true;
    try {
      const settings =
        source === "mirror"
          ? { visibility: "public", compress: false, compressed_only: false, connections: 3, partial_transfer: true, embed_credentials: true, listed: true, mirror: { url: mirror.trim(), import_events: true, import_hidden: true } }
          : undefined;
      const r = await post<{ slug: string; job: number | null }>("/repos", {
        slug,
        name: name.trim(),
        description,
        game,
        mode: source === "external" ? "external" : "managed",
        path: source === "external" ? path.trim() : undefined,
        settings,
      });
      open = false;
      toast(`Repository ${name.trim()} created`);
      // Mirrors start pulling right away.
      if (source === "mirror") {
        const m = await post<{ job: number }>(`/repos/${r.slug}/mirror`).catch(() => null);
        if (m?.job) openJob(m.job);
      }
      navigate(`/repos/${encodeURIComponent(r.slug)}${source === "upload" ? "?tab=files" : ""}`);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="New repository" persistent={busy}>
  <form class="stack" onsubmit={(e) => (e.preventDefault(), create())}>
    <div class="form-grid">
      <Field label="Name" required>{#snippet children(id)}<input {id} bind:value={name} placeholder="Main modpack" data-autofocus />{/snippet}</Field>
      <Field label="URL name" required error={slugErr} hint={slug ? `/r/${slug}` : "Used in the repository URL."}>
        {#snippet children(id)}<input {id} class="mono" bind:value={slug} oninput={() => (slugTouched = true)} placeholder="main" />{/snippet}
      </Field>
    </div>
    <Field label="Description" hint="Shown on the public page.">{#snippet children(id)}<input {id} bind:value={description} />{/snippet}</Field>
    <Field label="Game">
      {#snippet children()}<SegmentedControl bind:value={game} options={[{ value: "arma3", label: "Arma 3" }, { value: "reforger", label: "Arma Reforger" }]} />{/snippet}
    </Field>
    <Field label="Content">
      {#snippet children()}
        <SegmentedControl bind:value={source} options={[{ value: "upload", label: "Upload / push" }, { value: "mirror", label: "Mirror another repository" }, { value: "external", label: "Existing folder" }]} />
      {/snippet}
    </Field>
    {#if source === "upload"}
      <p class="small muted">Upload files in the web UI or push from the desktop app / CLI with an API token.</p>
    {:else if source === "mirror"}
      <Field label="Source repository" required hint="ArmA3Sync or ArmaXSync URL or auto-config URL. The first pull starts right away.">
        {#snippet children(id)}<input {id} class="mono" bind:value={mirror} placeholder="https://mods.example.com/arma3/main" />{/snippet}
      </Field>
    {:else}
      <Field label="Folder on the server" required hint="Content you edit over SFTP/SMB. Rebuild after changes.">
        {#snippet children(id)}<input {id} class="mono" bind:value={path} placeholder="/srv/arma/mods" />{/snippet}
      </Field>
    {/if}
    {#if error}<Alert tone="err">{error}</Alert>{/if}
  </form>
  {#snippet footer()}
    <Button onclick={() => (open = false)} disabled={busy}>Cancel</Button>
    <Button variant="primary" loading={busy} disabled={!valid} onclick={create}>Create repository</Button>
  {/snippet}
</Modal>
