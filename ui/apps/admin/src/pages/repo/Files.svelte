<script lang="ts">
  import { onMount } from "svelte";
  import { Modal, Progress, bytes, date, toast } from "@axs/kit";
  import { get, post, del, uploadFile, tusUpload } from "../../api";

  let { repo, onpublish }: { repo: any; onpublish: (job: number) => void } = $props();
  const slug = repo.slug;
  const external = repo.mode === "external";

  let path = $state("");
  let entries = $state<any[]>([]);
  let draft = $state<any>(null); // current draft changeset
  let uploading: { name: string; sent: number; total: number } | null = $state(null);
  let dragging = $state(false);
  let newFolderOpen = $state(false);
  let newFolder = $state("");
  let renameFrom = $state<string | null>(null);
  let renameTo = $state("");

  const crumbs = $derived(path ? path.split("/") : []);

  async function load() {
    entries = await get(`/repos/${slug}/tree?path=${encodeURIComponent(path)}`).catch(() => []);
  }
  async function loadDraft() {
    const list = await get(`/repos/${slug}/changesets?status=draft`);
    draft = list.length ? await get(`/changesets/${list[0].id}`) : null;
  }
  onMount(() => {
    load();
    if (!external) loadDraft();
  });

  function go(p: string) {
    path = p;
    load();
  }
  const join = (a: string, b: string) => (a ? `${a}/${b}` : b);

  async function ensureDraft(): Promise<number> {
    if (draft) return draft.id;
    const r = await post(`/repos/${slug}/changesets`, { note: "web" });
    draft = await get(`/changesets/${r.id}`);
    return r.id;
  }

  async function ops(list: any[]) {
    const id = await ensureDraft();
    draft = await post(`/changesets/${id}/ops`, list);
  }

  async function uploadFiles(files: { file: File; rel: string }[]) {
    if (!files.length) return;
    const id = await ensureDraft();
    const total = files.reduce((s, f) => s + f.file.size, 0);
    let done = 0;
    try {
      for (const f of files) {
        uploading = { name: f.rel, sent: done, total };
        if (/\.zip$/i.test(f.rel) && files.length === 1 && confirm(`Extract ${f.file.name} into ${path || "the repository root"}?`)) {
          const up = await tusUpload(f.file, (s) => (uploading = { name: f.rel, sent: done + s, total }));
          await post(`/changesets/${id}/extract`, { upload: up, dest: path });
        } else {
          await uploadFile(id, join(path, f.rel), f.file, (s) => (uploading = { name: f.rel, sent: done + s, total }));
        }
        done += f.file.size;
      }
      toast(`${files.length} file(s) staged`);
    } catch (e: any) {
      toast(e.message, "err", 8000);
    } finally {
      uploading = null;
      draft = await get(`/changesets/${id}`);
    }
  }

  function pick(dir: boolean) {
    const i = document.createElement("input");
    i.type = "file";
    i.multiple = true;
    if (dir) (i as any).webkitdirectory = true;
    i.onchange = () => uploadFiles([...(i.files ?? [])].map((f) => ({ file: f, rel: (f as any).webkitRelativePath || f.name })));
    i.click();
  }

  async function readEntry(entry: any, prefix: string, out: { file: File; rel: string }[]) {
    if (entry.isFile) {
      const f: File = await new Promise((res, rej) => entry.file(res, rej));
      out.push({ file: f, rel: prefix + f.name });
    } else if (entry.isDirectory) {
      const reader = entry.createReader();
      for (;;) {
        const batch: any[] = await new Promise((res, rej) => reader.readEntries(res, rej));
        if (!batch.length) break;
        for (const e of batch) await readEntry(e, `${prefix}${entry.name}/`, out);
      }
    }
  }

  async function drop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    if (external) return;
    const out: { file: File; rel: string }[] = [];
    const items = [...(e.dataTransfer?.items ?? [])].map((i) => i.webkitGetAsEntry?.()).filter(Boolean);
    for (const it of items) await readEntry(it, "", out);
    uploadFiles(out);
  }

  async function publish() {
    const r = await post(`/changesets/${draft.id}/commit`, { expected_revision: repo.revision, force: true });
    draft = null;
    onpublish(r.job);
  }

  async function discard() {
    if (!confirm("Discard all staged changes?")) return;
    await del(`/changesets/${draft.id}`);
    draft = null;
  }
</script>

{#if external}
  <div class="alert warn" style="margin-bottom:12px">This repository uses an existing folder on the server. Edit it directly (SFTP, SMB…) and click <b>Rebuild</b>.</div>
{/if}

<div class="stack">
  {#if draft && draft.ops.length}
    <div class="card stack">
      <div class="row">
        <h2 style="margin:0">Staged changes <span class="badge info">{draft.ops.length}</span></h2>
        <div class="spacer"></div>
        <button class="btn" onclick={discard}>Discard</button>
        <button class="btn primary" onclick={publish}>Publish new revision</button>
      </div>
      <div class="log" style="max-height:160px">{draft.ops.map((o: any) => `${o.op.padEnd(6)} ${o.path}${o.size !== undefined ? "  (" + bytes(o.size) + ")" : ""}`).join("\n")}</div>
      <div class="small muted">Players keep downloading the published revision until you publish.</div>
    </div>
  {/if}

  <div class="card flush" role="region" aria-label="Files" ondragover={(e) => { e.preventDefault(); dragging = !external; }} ondragleave={() => (dragging = false)} ondrop={drop} class:drag={dragging}>
    <div class="row toolbar">
      <div class="crumbs">
        <button class="btn ghost small" onclick={() => go("")}>{repo.slug}</button>
        {#each crumbs as c, i}/<button class="btn ghost small" onclick={() => go(crumbs.slice(0, i + 1).join("/"))}>{c}</button>{/each}
      </div>
      <div class="spacer"></div>
      {#if !external}
        <button class="btn small" onclick={() => (newFolderOpen = true)}>New folder</button>
        <button class="btn small" onclick={() => pick(true)}>Upload folder</button>
        <button class="btn small primary" onclick={() => pick(false)}>Upload files</button>
      {/if}
    </div>
    {#if uploading}
      <div style="padding:10px 12px"><Progress value={uploading.sent} max={uploading.total} label={`Uploading ${uploading.name} — ${bytes(uploading.sent)} / ${bytes(uploading.total)}`} /></div>
    {/if}
    <table>
      <thead><tr><th>Name</th><th>Size</th><th class="hide-sm">Modified</th><th></th></tr></thead>
      <tbody>
        {#if path}<tr class="clickable" onclick={() => go(crumbs.slice(0, -1).join("/"))}><td colspan="4">⬑ ..</td></tr>{/if}
        {#each entries as e}
          <tr class:clickable={e.dir} onclick={() => e.dir && go(join(path, e.name))}>
            <td>{e.dir ? "📁" : "📄"} {e.name} {#if e.addon}<span class="badge info">addon</span>{/if}</td>
            <td>{e.dir ? "" : bytes(e.size)}</td>
            <td class="hide-sm">{date(e.modified)}</td>
            <td style="text-align:right">
              {#if !external}
                <button class="btn ghost small" onclick={(ev) => { ev.stopPropagation(); renameFrom = join(path, e.name); renameTo = renameFrom; }}>Rename</button>
                <button class="btn ghost small danger" onclick={(ev) => { ev.stopPropagation(); ops([{ op: "delete", path: join(path, e.name) }]); }}>Delete</button>
              {/if}
            </td>
          </tr>
        {:else}
          <tr><td colspan="4" class="empty">{external ? "Empty folder." : "Empty — drop files or folders here (zip archives can be extracted)."}</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<Modal bind:open={newFolderOpen} title="New folder">
  <label class="field">Name <input bind:value={newFolder} /></label>
  {#snippet footer()}
    <button class="btn primary" onclick={async () => { await ops([{ op: "mkdir", path: join(path, newFolder) }]); newFolderOpen = false; newFolder = ""; }}>Create</button>
  {/snippet}
</Modal>

{#if renameFrom}
  <Modal open={true} title="Rename / move">
    <label class="field">New path <input bind:value={renameTo} /></label>
    {#snippet footer()}
      <button class="btn" onclick={() => (renameFrom = null)}>Cancel</button>
      <button class="btn primary" onclick={async () => { await ops([{ op: "move", from: renameFrom, to: renameTo }]); renameFrom = null; }}>Stage move</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .toolbar { padding: 8px 10px; border-bottom: 1px solid var(--line); }
  .crumbs { display: flex; align-items: center; flex-wrap: wrap; gap: 2px; }
  .drag { outline: 2px dashed var(--accent); outline-offset: -4px; }
</style>
