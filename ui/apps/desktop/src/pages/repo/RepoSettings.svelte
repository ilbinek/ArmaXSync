<script lang="ts">
  import { Section, Field, PasswordInput, PathInput, Switch, TagListInput, SaveBar, Button, useUnsavedGuard, snapshot, toast, errorText, navigate, confirmDialog } from "@axs/kit";
  import { Link, FolderDown, Gauge, Wrench, Trash2 } from "@lucide/svelte";
  import { call, pickFolder, type RepoConfig } from "../../api";
  import { app, refresh } from "../../store.svelte";

  let { repo }: { repo: RepoConfig } = $props();

  // svelte-ignore state_referenced_locally
  let edit = $state(structuredClone($state.snapshot(repo)) as RepoConfig);
  // svelte-ignore state_referenced_locally
  let saved = $state(snapshot(repo));
  let saving = $state(false);
  let error = $state<string | null>(null);

  const dirty = $derived(snapshot(edit) !== saved);
  useUnsavedGuard(() => dirty);

  const nameTaken = $derived(edit.name.trim() !== repo.name && !!app.state?.repos.some((r) => r.name.toLowerCase() === edit.name.trim().toLowerCase()));
  const urlBad = $derived(!/^(https?|ftp):\/\//i.test(edit.url.trim()));
  const valid = $derived(!!edit.name.trim() && !nameTaken && !urlBad);

  let speedMb = $state(0);
  $effect.pre(() => {
    speedMb = edit.max_speed / 1e6;
  });

  async function save() {
    if (!valid) return;
    saving = true;
    error = null;
    try {
      const r = { ...$state.snapshot(edit), name: edit.name.trim(), url: edit.url.trim() };
      await call("repo_save", { repo: r, original: repo.name });
      saved = snapshot(edit);
      await refresh();
      toast("Settings saved");
      if (r.name !== repo.name) navigate(`/repos/${encodeURIComponent(r.name)}?tab=settings`);
    } catch (e) {
      error = errorText(e);
    } finally {
      saving = false;
    }
  }

  function discard() {
    edit = JSON.parse(saved);
    error = null;
  }

  async function remove() {
    if (!(await confirmDialog({ title: `Remove ${repo.name}?`, message: "The repository is removed from ArmaXSync. Downloaded files stay on disk.", confirmLabel: "Remove", danger: true }))) return;
    try {
      saved = snapshot(edit); // leaving is intended
      await call("repo_remove", { name: repo.name });
      await refresh();
      navigate("/repos");
    } catch (e) {
      toast(errorText(e), "err");
    }
  }
</script>

<div class="stack loose narrowish">
  <Section title="Connection" icon={Link}>
    <div class="form-grid">
      <Field label="Name" required error={nameTaken ? "Another repository has this name" : !edit.name.trim() ? "Required" : null}>
        {#snippet children(id)}<input {id} bind:value={edit.name} />{/snippet}
      </Field>
      <Field label="Repository URL" required error={urlBad ? "Use an http://, https:// or ftp:// URL" : null}>
        {#snippet children(id)}<input {id} bind:value={edit.url} class="mono" spellcheck="false" />{/snippet}
      </Field>
      <Field label="Login" hint="Only for private repositories.">
        {#snippet children(id)}<input {id} bind:value={edit.login} placeholder="anonymous" autocomplete="off" />{/snippet}
      </Field>
      <Field label="Password">
        {#snippet children(id)}<PasswordInput {id} bind:value={edit.password} />{/snippet}
      </Field>
    </div>
  </Section>

  <Section title="Downloads" icon={FolderDown}>
    <Field label="Download folder" hint="Its addons are found automatically when you launch the game.">
      {#snippet children(id)}<PathInput {id} bind:value={edit.destination} browse={() => pickFolder("Download folder")} />{/snippet}
    </Field>
    <div class="stack tight">
      <Switch bind:checked={edit.notify} label="Notify me about updates" />
      <Switch bind:checked={edit.auto} label="Download updates automatically" hint="Starts as soon as a check finds a new revision." />
      <Switch bind:checked={edit.exact_match} label="Exact match" hint="Also delete extra files in the download folder root that are not in the repository." />
      <Switch bind:checked={edit.auto_discover} label="Reuse addons from other folders" hint="Copy identical addons you already have instead of downloading them again." />
    </div>
    <Field label="Keep my extra files in these folders" hint="Relative to the download folder, e.g. @ace/userconfig. Press Enter after each.">
      {#snippet children(id)}<TagListInput {id} bind:values={edit.keep_extras} placeholder="@mod/userconfig" />{/snippet}
    </Field>
  </Section>

  <Section title="Performance" icon={Gauge}>
    <div class="form-grid">
      <Field label="Connections" hint="0 uses the repository's recommendation.">
        {#snippet children(id)}<input {id} type="number" min="0" max="10" bind:value={edit.connections} />{/snippet}
      </Field>
      <Field label="Speed limit (MB/s)" hint="0 = unlimited.">
        {#snippet children(id)}<input {id} type="number" min="0" step="0.5" bind:value={speedMb} oninput={() => (edit.max_speed = Math.round((speedMb || 0) * 1e6))} />{/snippet}
      </Field>
      <Field label="Connect timeout (s)" hint="Empty = default.">
        {#snippet children(id)}<input {id} type="number" min="0" bind:value={edit.connect_timeout_secs} />{/snippet}
      </Field>
      <Field label="Read timeout (s)" hint="Empty = default.">
        {#snippet children(id)}<input {id} type="number" min="0" bind:value={edit.read_timeout_secs} />{/snippet}
      </Field>
    </div>
  </Section>

  <Section title="Advanced" icon={Wrench}>
    <Switch bind:checked={edit.prefer_modern} label="Prefer ArmaXSync metadata" hint="Faster checks when the server provides them; falls back to ArmA3Sync files." />
    <Switch bind:checked={edit.accept_invalid_certs} label="Accept invalid HTTPS certificates" hint="Not recommended: only for servers with self-signed certificates you trust." />
  </Section>

  <Section title="Remove repository" description="Stops checking and syncing. Downloaded files stay on disk.">
    <div><Button variant="danger" icon={Trash2} onclick={remove}>Remove {repo.name}</Button></div>
  </Section>

  <SaveBar {dirty} {saving} {error} onsave={save} ondiscard={discard} message={valid ? "Unsaved changes" : "Fix the highlighted fields"} />
</div>

<style>
  .narrowish { max-width: 900px; }
</style>
