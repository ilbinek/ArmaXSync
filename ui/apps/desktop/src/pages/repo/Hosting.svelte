<script lang="ts">
  import { onDestroy } from "svelte";
  import { Section, Field, PathInput, PasswordInput, Switch, SaveBar, Button, Progress, Alert, useUnsavedGuard, snapshot, toast, errorText, bytes, confirmWithOption } from "@axs/kit";
  import { Hammer, Upload, FolderCog, Info } from "@lucide/svelte";
  import { call, on, pickFolder, type RepoConfig, type AdminSettings } from "../../api";
  import { refresh } from "../../store.svelte";

  let { repo }: { repo: RepoConfig } = $props();

  const blank: AdminSettings = { path: null, public_url: null, compress: false, compressed_only: false, connections: 3, partial_transfer: true, embed_credentials: true, exclude: [], hidden: [], servers: [], push_server: null, push_repo: null, push_token: null };
  // svelte-ignore state_referenced_locally
  let admin = $state<AdminSettings>(structuredClone($state.snapshot(repo.admin) ?? blank));
  // svelte-ignore state_referenced_locally
  let saved = $state(snapshot(repo.admin ?? blank));
  let saving = $state(false);
  const dirty = $derived(snapshot(admin) !== saved);
  useUnsavedGuard(() => dirty);

  let busy = $state<"build" | "push" | null>(null);
  let prog = $state<{ done: number; total: number; label: string } | null>(null);
  let result = $state<{ tone: "ok" | "err"; title: string; text: string } | null>(null);

  const unlisten: (() => void)[] = [];
  on<[number, number]>("build-progress", ([d, t]) => busy === "build" && (prog = { done: d, total: t, label: `Hashing ${bytes(d)} of ${bytes(t)}` })).then((u) => unlisten.push(u));
  let pushTotal = 0;
  let pushSent = 0;
  on<any>("push-progress", (p) => {
    if (busy !== "push") return;
    if (p.phase === "hashing") prog = { done: p.done, total: p.total, label: `Comparing files ${p.done} / ${p.total}` };
    else if (p.phase === "planned") ((pushTotal = p.total), (pushSent = 0), (prog = { done: 0, total: p.total, label: `Uploading ${bytes(p.total)}` }));
    else if (p.phase === "uploading") prog = { done: pushSent, total: pushTotal, label: `Uploading ${p.path}` };
    else if (p.phase === "uploaded") ((pushSent += p.bytes), (prog = { done: pushSent, total: pushTotal, label: `Uploaded ${bytes(pushSent)} of ${bytes(pushTotal)}` }));
    else if (p.phase === "publishing") prog = { done: 0, total: 0, label: `Publishing on the server (job #${p.job})` };
  }).then((u) => unlisten.push(u));
  onDestroy(() => unlisten.forEach((u) => u()));

  async function save() {
    saving = true;
    try {
      await call("repo_save", { repo: { ...$state.snapshot(repo), admin: $state.snapshot(admin) }, original: repo.name });
      saved = snapshot(admin);
      await refresh();
      toast("Hosting settings saved");
    } catch (e) {
      toast(errorText(e), "err");
    } finally {
      saving = false;
    }
  }

  async function build() {
    busy = "build";
    result = null;
    prog = { done: 0, total: 0, label: "Scanning files…" };
    try {
      const r: any = await call("build_repo", { name: repo.name });
      result = { tone: "ok", title: `Revision ${r.revision} built`, text: `${r.files.toLocaleString()} files, ${bytes(r.total_size)}; ${r.hashed_files} hashed in ${(r.duration_ms / 1000 || 0).toFixed(1)} s. Upload the folder to your web server.` };
    } catch (e) {
      result = { tone: "err", title: "Build failed", text: errorText(e) };
    } finally {
      busy = null;
      prog = null;
    }
  }

  async function push() {
    const c = await confirmWithOption({
      title: "Push to server?",
      message: `Uploads the changes in ${admin.path} to ${admin.push_server} (repository ${admin.push_repo}) and publishes a new revision.`,
      checkbox: { label: "Also delete files on the server that are not in the local folder", checked: true },
      confirmLabel: "Push",
    });
    if (!c) return;
    busy = "push";
    result = null;
    prog = { done: 0, total: 0, label: "Connecting…" };
    try {
      const r: any = await call("push_repo", { name: repo.name, deleteMissing: c.checked });
      result = { tone: "ok", title: `Published${r.revision != null ? ` revision ${r.revision}` : ""}`, text: `${bytes(r.bytes_sent)} sent for ${bytes(r.bytes_total)} of changes · ${r.delta_files} delta, ${r.full_files} full, ${r.deleted} deleted.` };
    } catch (e) {
      result = { tone: "err", title: "Push failed", text: errorText(e) };
    } finally {
      busy = null;
      prog = null;
    }
  }
</script>

<div class="stack loose narrowish">
  <Alert tone="info" icon={Info}>For community admins. Build this repository from a local folder and upload it to any web server, or push it to an ArmaXSync server.</Alert>

  <Section title="Source folder" icon={FolderCog}>
    <Field label="Repository folder" required hint="The folder holding the @addons.">
      {#snippet children(id)}<PathInput {id} bind:value={admin.path} browse={() => pickFolder("Repository folder")} />{/snippet}
    </Field>
    <div class="form-grid">
      <Field label="Public URL" hint="Defaults to the repository URL.">
        {#snippet children(id)}<input {id} bind:value={admin.public_url} placeholder={repo.url} class="mono" />{/snippet}
      </Field>
      <Field label="Connections per client">
        {#snippet children(id)}<input {id} type="number" min="1" max="10" bind:value={admin.connections} />{/snippet}
      </Field>
    </div>
    <div class="stack tight">
      <Switch bind:checked={admin.compress} label="Compressed PBOs" hint="Also publish .pbo.zip files for slow connections." />
      <Switch bind:checked={admin.partial_transfer} label="Partial transfers" hint="Let clients download only the changed parts of files." />
      <Switch bind:checked={admin.embed_credentials} label="Embed login in auto-config" />
    </div>
  </Section>

  <Section title="Build locally" icon={Hammer} description="Writes the ArmA3Sync and ArmaXSync metadata into the folder.">
    {#if busy === "build" && prog}<Progress value={prog.done} max={prog.total} indeterminate={!prog.total} label={prog.label} />{/if}
    {#snippet footer()}
      {#if dirty}<span class="small muted grow">Save your changes first.</span>{/if}
      <Button variant="primary" icon={Hammer} loading={busy === "build"} disabled={!!busy || dirty || !admin.path} onclick={build}>Build repository</Button>
    {/snippet}
  </Section>

  <Section title="Push to an ArmaXSync server" icon={Upload} description="Delta upload: only changed parts of files are sent.">
    <div class="form-grid">
      <Field label="Server URL">{#snippet children(id)}<input {id} bind:value={admin.push_server} placeholder="https://mods.example.com" class="mono" />{/snippet}</Field>
      <Field label="Server repository" hint="The repository's slug on the server.">{#snippet children(id)}<input {id} bind:value={admin.push_repo} placeholder="main" class="mono" />{/snippet}</Field>
    </div>
    <Field label="API token" hint="Create one under API tokens in the server's admin page.">
      {#snippet children(id)}<PasswordInput {id} bind:value={admin.push_token} placeholder="axs_…" />{/snippet}
    </Field>
    {#if busy === "push" && prog}<Progress value={prog.done} max={prog.total} indeterminate={!prog.total} label={prog.label} />{/if}
    {#snippet footer()}
      {#if dirty}<span class="small muted grow">Save your changes first.</span>{/if}
      <Button variant="primary" icon={Upload} loading={busy === "push"} disabled={!!busy || dirty || !admin.path || !admin.push_server || !admin.push_repo || !admin.push_token} onclick={push}>Push changes</Button>
    {/snippet}
  </Section>

  {#if result}<Alert tone={result.tone} title={result.title} dismissible>{result.text}</Alert>{/if}

  <SaveBar {dirty} {saving} onsave={save} ondiscard={() => (admin = JSON.parse(saved))} />
</div>

<style>
  .narrowish { max-width: 900px; }
</style>
