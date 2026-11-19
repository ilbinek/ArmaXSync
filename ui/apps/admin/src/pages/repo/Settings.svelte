<script lang="ts">
  import { onMount } from "svelte";
  import { Section, Field, Switch, SegmentedControl, TagListInput, PasswordInput, SaveBar, Button, Alert, useUnsavedGuard, snapshot, toast, errorText } from "@axs/kit";
  import { Info, Download, FolderX, Server as ServerIcon, Plus, X, Lock, Globe } from "@lucide/svelte";
  import { get, patch, type RepoDetail, type RepoSettings, type ServerDef } from "../../api";
  import { openJob } from "../../session.svelte";

  let { repo, onsaved }: { repo: RepoDetail; onsaved: () => void } = $props();

  // The editable subset; mirror and events have their own tabs.
  type Form = { name: string; description: string; settings: Omit<RepoSettings, "mirror" | "events"> };
  const toForm = (r: RepoDetail): Form => {
    const { mirror: _m, events: _e, ...rest } = structuredClone($state.snapshot(r.settings)) as RepoSettings;
    return { name: r.name, description: r.description, settings: rest };
  };
  // svelte-ignore state_referenced_locally
  let form = $state<Form>(toForm(repo));
  // svelte-ignore state_referenced_locally
  let saved = $state(snapshot(toForm(repo)));
  let saving = $state(false);
  let error = $state<string | null>(null);
  let creds = $state<number | null>(null);
  let mbps = $state(0);
  $effect.pre(() => {
    mbps = form.settings.bandwidth_limit / 1e6;
  });

  onMount(async () => {
    creds = await get<any[]>(`/repos/${repo.slug}/credentials`).then((c) => c.length).catch(() => null);
  });

  const dirty = $derived(snapshot(form) !== saved);
  useUnsavedGuard(() => dirty);
  const s = $derived(form.settings);

  function changedKeys(): Record<string, unknown> {
    const old = JSON.parse(saved) as Form;
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries($state.snapshot(form.settings))) {
      if (JSON.stringify(v) !== JSON.stringify((old.settings as any)[k])) out[k] = v;
    }
    return out;
  }

  async function save() {
    saving = true;
    error = null;
    try {
      const old = JSON.parse(saved) as Form;
      const settings = changedKeys();
      if (settings.servers) settings.servers = (settings.servers as ServerDef[]).filter((x) => x.name.trim() && x.host.trim());
      const body: Record<string, unknown> = {};
      if (form.name !== old.name) body.name = form.name;
      if (form.description !== old.description) body.description = form.description;
      if (Object.keys(settings).length) body.settings = settings;
      const r = await patch<{ job?: number | null }>(`/repos/${repo.slug}`, body);
      saved = snapshot(form);
      toast("Settings saved", "ok", { detail: r.job ? "Publishing new metadata for players." : undefined });
      if (r.job) openJob(r.job);
      onsaved();
    } catch (e) {
      error = errorText(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="stack loose narrowish">
  <Section title="General" icon={Info}>
    <div class="form-grid">
      <Field label="Name" required>{#snippet children(id)}<input {id} bind:value={form.name} />{/snippet}</Field>
      <Field label="Description" hint="Shown on the public page.">{#snippet children(id)}<input {id} bind:value={form.description} />{/snippet}</Field>
    </div>
    <Field label="Who can download">
      {#snippet children()}
        <SegmentedControl bind:value={form.settings.visibility} options={[{ value: "public", label: "Everyone", icon: Globe }, { value: "private", label: "Only with a login", icon: Lock }]} />
      {/snippet}
    </Field>
    {#if s.visibility === "private" && creds === 0}
      <Alert tone="warn" title="Nobody can download this repository">It is private but has no download logins. Add one under <a href={`#/repos/${repo.slug}?tab=access`}>Access</a>.</Alert>
    {/if}
    <Switch bind:checked={form.settings.listed} label="List on the public page" hint="Unlisted repositories still work for anyone with the link." />
  </Section>

  <Section title="Downloads" icon={Download} description="Changing these publishes new metadata (a new revision for players).">
    <div class="form-grid">
      <Field label="Connections per player" hint="Parallel downloads recommended to clients (1–10).">
        {#snippet children(id)}<input {id} type="number" min="1" max="10" bind:value={form.settings.connections} />{/snippet}
      </Field>
      <Field label="Bandwidth limit (MB/s)" hint="For this repository. 0 = unlimited.">
        {#snippet children(id)}<input {id} type="number" min="0" step="0.5" bind:value={mbps} oninput={() => (form.settings.bandwidth_limit = Math.round((mbps || 0) * 1e6))} />{/snippet}
      </Field>
      <Field label="Concurrent downloads" hint="Files served at once. 0 = unlimited.">
        {#snippet children(id)}<input {id} type="number" min="0" bind:value={form.settings.max_downloads} />{/snippet}
      </Field>
    </div>
    <div class="stack tight">
      <Switch bind:checked={form.settings.partial_transfer} label="Partial transfers" hint="Clients download only the changed parts of files (zsync)." />
      <Switch bind:checked={form.settings.compress} label="Compressed PBOs" hint="Also publish .pbo.zip files; useful for slow connections." />
      <Switch bind:checked={form.settings.compressed_only} disabled={!s.compress} label="Only compressed PBOs" hint="Clients must download the .zip versions." />
      <Switch bind:checked={form.settings.embed_credentials} label="Embed a login in the auto-config" hint="Legacy behaviour: the auto-config file contains the embedded download login." />
    </div>
  </Section>

  <Section title="Content rules" icon={FolderX} description="Paths relative to the repository root. Press Enter after each.">
    <Field label="Folders players keep" hint="Clients don't delete extra files in these, e.g. @mod/userconfig.">
      {#snippet children(id)}<TagListInput {id} bind:values={form.settings.hidden} placeholder="@ace/userconfig" />{/snippet}
    </Field>
    <Field label="Excluded files" hint="Never published, e.g. *.bak or @mod/source.">
      {#snippet children(id)}<TagListInput {id} bind:values={form.settings.exclude} placeholder="*.bak" />{/snippet}
    </Field>
  </Section>

  <Section title="Game servers" icon={ServerIcon} description="Offered to players in the desktop app (join with one click).">
    {#each form.settings.servers as srv, i}
      <div class="srv card raised">
        <div class="srvgrid">
          <Field label="Name">{#snippet children(id)}<input {id} bind:value={srv.name} placeholder="Main server" />{/snippet}</Field>
          <Field label="Address">{#snippet children(id)}<input {id} class="mono" bind:value={srv.host} placeholder="play.example.com" />{/snippet}</Field>
          <Field label="Port">{#snippet children(id)}<input {id} type="number" min="1" max="65535" bind:value={srv.port} />{/snippet}</Field>
          <Field label="Password">{#snippet children(id)}<PasswordInput {id} bind:value={srv.password} />{/snippet}</Field>
          <Field label="Join with event">
            {#snippet children(id)}
              <select {id} bind:value={srv.modset}>
                <option value={null}>Whole repository</option>
                {#each repo.settings.events as e}<option value={e.name}>{e.name}</option>{/each}
              </select>
            {/snippet}
          </Field>
        </div>
        <button class="btn ghost icon sm rm" aria-label={`Remove ${srv.name || "server"}`} onclick={() => form.settings.servers.splice(i, 1)}><X size={15} /></button>
      </div>
    {/each}
    <div><Button size="sm" icon={Plus} onclick={() => form.settings.servers.push({ name: "", host: "", port: 2302, password: null, modset: null })}>Add server</Button></div>
  </Section>

  <SaveBar {dirty} {saving} {error} onsave={save} ondiscard={() => (form = JSON.parse(saved))} />
</div>

<style>
  .narrowish { max-width: 900px; }
  .srv { position: relative; padding: 12px 44px 12px 14px; }
  .srvgrid { display: grid; grid-template-columns: 1.2fr 1.4fr 90px 1fr 1.2fr; gap: 10px; }
  @media (max-width: 900px) { .srvgrid { grid-template-columns: 1fr 1fr; } }
  .rm { position: absolute; top: 8px; right: 8px; }
</style>
