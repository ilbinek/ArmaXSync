<script lang="ts">
  import { onMount } from "svelte";
  import { Button, Modal, Field, Alert, Progress, Skeleton, EmptyState, Badge, Menu, bytes, date, toast, toastError, errorText, confirmDialog, promptDialog, useUnsavedGuard } from "@axs/kit";
  import { Folder, FolderPlus, File as FileIcon, Upload, FolderUp, CornerLeftUp, Pencil, Trash2, Undo2, Send, X, CircleCheck, CircleAlert, LoaderCircle, Package, HardDrive, ChevronRight } from "@lucide/svelte";
  import { get, post, del, uploadFile, tusUpload, ApiError, type RepoDetail } from "../../api";
  import { openJob } from "../../session.svelte";

  let { repo }: { repo: RepoDetail } = $props();
  // svelte-ignore state_referenced_locally
  const slug = repo.slug;
  // svelte-ignore state_referenced_locally
  const external = repo.mode === "external";

  type Entry = { name: string; dir: boolean; size: number; modified: number; addon?: boolean };
  type Op = { op: "put" | "delete" | "mkdir"; path: string; size?: number };
  type Draft = { id: number; ops: Op[]; base_revision: number | null };

  let path = $state("");
  let entries = $state<Entry[] | null>(null);
  let listError = $state<string | null>(null);
  let draft = $state<Draft | null>(null);
  let dragging = $state(false);

  // ------------------------------------------------------------- loading --
  async function load() {
    listError = null;
    entries = null;
    try {
      entries = await get<Entry[]>(`/repos/${slug}/tree?path=${encodeURIComponent(path)}`);
    } catch (e) {
      listError = errorText(e);
      entries = [];
    }
  }
  async function loadDraft() {
    const list = await get<{ id: number }[]>(`/repos/${slug}/changesets?status=draft`).catch(() => []);
    draft = list.length ? await get<Draft>(`/changesets/${list[0].id}`) : null;
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
  const crumbs = $derived(path ? path.split("/") : []);

  async function ensureDraft(): Promise<number> {
    if (draft) return draft.id;
    const r = await post<{ id: number }>(`/repos/${slug}/changesets`, { note: "web" });
    draft = await get<Draft>(`/changesets/${r.id}`);
    return r.id;
  }
  async function ops(list: object[]) {
    try {
      const id = await ensureDraft();
      draft = await post<Draft>(`/changesets/${id}/ops`, list);
    } catch (e) {
      toastError(e);
    }
  }
  async function unstage(paths: string[]) {
    if (!draft) return;
    try {
      draft = await post<Draft>(`/changesets/${draft.id}/ops/remove`, { paths });
    } catch (e) {
      toastError(e);
    }
  }

  // ------------------------------------------------- staged overlay view --
  const staged = $derived(draft?.ops ?? []);
  const isDeleted = (p: string) => staged.some((o) => o.op === "delete" && (p === o.path || p.startsWith(o.path + "/")));
  const opFor = (p: string) => staged.find((o) => o.path === p && o.op !== "delete");
  type Row = Entry & { full: string; state: "live" | "new" | "replaced" | "deleted"; virtual?: boolean };
  const rows = $derived.by((): Row[] => {
    if (!entries) return [];
    const live: Row[] = entries.map((e) => {
      const full = join(path, e.name);
      const o = opFor(full);
      return { ...e, full, state: isDeleted(full) ? "deleted" : o && !e.dir ? "replaced" : "live", size: o?.size ?? e.size };
    });
    const names = new Set(entries.map((e) => e.name));
    const extra = new Map<string, Row>();
    const prefix = path ? path + "/" : "";
    for (const o of staged) {
      if (o.op === "delete" || !o.path.startsWith(prefix)) continue;
      const rest = o.path.slice(prefix.length);
      const [first, ...more] = rest.split("/");
      if (!first || names.has(first)) continue;
      const dir = more.length > 0 || o.op === "mkdir";
      const cur = extra.get(first);
      extra.set(first, { name: first, dir, size: (cur?.size ?? 0) + (o.size ?? 0), modified: 0, full: join(path, first), state: "new", virtual: true });
    }
    return [...live, ...extra.values()].sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name));
  });

  // ------------------------------------------------------------- uploads --
  type Item = { rel: string; file: File; sent: number; status: "queued" | "uploading" | "done" | "error" | "cancelled"; error?: string; ctrl?: AbortController };
  let queue = $state<Item[]>([]);
  let showQueue = $state(false);
  const uploading = $derived(queue.some((q) => q.status === "queued" || q.status === "uploading"));
  const qTotal = $derived(queue.reduce((s, q) => s + q.file.size, 0));
  const qSent = $derived(queue.reduce((s, q) => s + (q.status === "done" ? q.file.size : q.sent), 0));
  useUnsavedGuard(() => uploading, "Uploads are still running. Leave and cancel them?");

  async function uploadFiles(files: { file: File; rel: string }[]) {
    if (!files.length) return;
    let id: number;
    try {
      id = await ensureDraft();
    } catch (e) {
      return toastError(e);
    }
    // A single zip can be extracted on the server.
    if (files.length === 1 && /\.zip$/i.test(files[0].rel)) {
      const extract = await confirmDialog({
        title: `Extract ${files[0].file.name}?`,
        message: `Unpack the archive into ${path || "the repository root"} instead of uploading the .zip file itself.`,
        confirmLabel: "Extract",
        cancelLabel: "Upload as a file",
      });
      if (extract) {
        const it: Item = { rel: files[0].rel + " (extract)", file: files[0].file, sent: 0, status: "uploading", ctrl: new AbortController() };
        queue = [...queue, it];
        showQueue = true;
        const item = queue[queue.length - 1];
        try {
          const up = await tusUpload(it.file, (s) => (item.sent = s), item.ctrl!.signal);
          await post(`/changesets/${id}/extract`, { upload: up, dest: path });
          item.status = "done";
        } catch (e) {
          item.status = (e as any)?.name === "AbortError" ? "cancelled" : "error";
          item.error = errorText(e);
        }
        draft = await get<Draft>(`/changesets/${id}`);
        return;
      }
    }
    const start = queue.length;
    queue = [...queue, ...files.map((f) => ({ rel: f.rel, file: f.file, sent: 0, status: "queued" as const }))];
    showQueue = true;
    for (let i = start; i < queue.length; i++) {
      const item = queue[i];
      if (item.status !== "queued") continue;
      item.status = "uploading";
      item.ctrl = new AbortController();
      try {
        await uploadFile(id, join(path, item.rel), item.file, (s) => (item.sent = s), item.ctrl.signal);
        item.status = "done";
      } catch (e) {
        item.status = (e as any)?.name === "AbortError" ? "cancelled" : "error";
        item.error = errorText(e);
      }
    }
    draft = await get<Draft>(`/changesets/${id}`);
    const failed = queue.slice(start).filter((q) => q.status === "error").length;
    if (failed) toast(`${failed} upload(s) failed`, "err");
    else toast(`${files.length} file(s) staged`, "ok", { detail: "Publish to make them available to players." });
  }
  function cancelUploads() {
    for (const q of queue) {
      if (q.status === "queued") q.status = "cancelled";
      if (q.status === "uploading") q.ctrl?.abort();
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

  // --------------------------------------------------------------- actions --
  async function newFolder() {
    const name = await promptDialog({ title: "New folder", label: "Folder name", placeholder: "@my_mod", confirmLabel: "Create", validate: (v) => (/[\\/]/.test(v) ? "No slashes" : rows.some((r) => r.name === v) ? "Already exists" : null) });
    if (name) ops([{ op: "mkdir", path: join(path, name) }]);
  }
  async function remove(r: Row) {
    await ops([{ op: "delete", path: r.full }]);
    toast(`${r.name} will be deleted when you publish`, "info", { action: { label: "Undo", run: () => unstage([r.full]) } });
  }

  let renameRow = $state<Row | null>(null);
  let renameName = $state("");
  let renameDir = $state("");
  function startRename(r: Row) {
    renameRow = r;
    renameName = r.name;
    renameDir = path;
  }
  async function doRename() {
    if (!renameRow) return;
    const to = join(renameDir.replace(/^\/+|\/+$/g, ""), renameName.trim());
    if (to !== renameRow.full) await ops([{ op: "move", from: renameRow.full, to }]);
    renameRow = null;
  }

  // ---------------------------------------------------------------- publish --
  let reviewOpen = $state(false);
  let publishing = $state(false);
  let conflict = $state<string | null>(null);
  const summary = $derived({
    put: staged.filter((o) => o.op === "put").length,
    del: staged.filter((o) => o.op === "delete").length,
    mkdir: staged.filter((o) => o.op === "mkdir").length,
    bytes: staged.reduce((s, o) => s + (o.size ?? 0), 0),
  });
  async function publish(force = false) {
    if (!draft) return;
    publishing = true;
    try {
      const r = await post<{ job: number }>(`/changesets/${draft.id}/commit`, { expected_revision: draft.base_revision ?? repo.revision, force });
      draft = null;
      reviewOpen = false;
      conflict = null;
      openJob(r.job);
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) conflict = e.message;
      else toastError(e);
    } finally {
      publishing = false;
    }
  }
  async function discard() {
    if (!draft || !(await confirmDialog({ title: "Discard all staged changes?", message: `${staged.length} change(s) and the uploaded files are thrown away.`, confirmLabel: "Discard", danger: true }))) return;
    try {
      await del(`/changesets/${draft.id}`);
      draft = null;
    } catch (e) {
      toastError(e);
    }
  }
  const opIcon = (o: Op) => (o.op === "delete" ? Trash2 : o.op === "mkdir" ? FolderPlus : Upload);
</script>

<div class="stack">
  {#if external}
    <Alert tone="info" icon={HardDrive} title="Existing folder">This repository serves <code>{repo.path}</code>. Edit it directly (SFTP, SMB…) and click <b>Rebuild</b> to publish the changes.</Alert>
  {/if}

  {#if staged.length}
    <section class="card staged">
      <div class="row">
        <span class="sicon"><Send size={16} /></span>
        <div class="grow">
          <div class="strong">{staged.length} staged change{staged.length > 1 ? "s" : ""}</div>
          <div class="xs muted">Players keep the published revision until you publish.</div>
        </div>
        <Button variant="ghost" size="sm" onclick={discard}>Discard all</Button>
        <Button variant="primary" icon={Send} disabled={uploading} onclick={() => (reviewOpen = true)}>Review & publish</Button>
      </div>
      <ul class="ops">
        {#each staged as o (o.op + o.path)}
          {@const Icon = opIcon(o)}
          <li class={o.op}>
            <Icon size={14} />
            <span class="mono small grow ellipsis" title={o.path}>{o.path}</span>
            {#if o.size != null}<span class="xs muted tabular">{bytes(o.size)}</span>{/if}
            <button class="btn ghost sm" onclick={() => unstage([o.path])} aria-label={`Undo ${o.op} ${o.path}`}><Undo2 size={13} />Undo</button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if queue.length && showQueue}
    <section class="card stack tight">
      <div class="row">
        <strong class="grow">{uploading ? "Uploading" : "Uploads"} · {queue.filter((q) => q.status === "done").length} of {queue.length}</strong>
        {#if uploading}<Button size="sm" variant="ghost" icon={X} onclick={cancelUploads}>Cancel</Button>{:else}<Button size="sm" variant="ghost" onclick={() => ((queue = []), (showQueue = false))}>Clear</Button>{/if}
      </div>
      {#if uploading}<Progress value={qSent} max={qTotal} label={`${bytes(qSent)} of ${bytes(qTotal)}`} />{/if}
      <ul class="queue">
        {#each queue.filter((q) => q.status !== "done" || !uploading).slice(-8) as q}
          <li>
            {#if q.status === "done"}<CircleCheck size={14} class="ok-text" />{:else if q.status === "error"}<CircleAlert size={14} class="err-text" />{:else if q.status === "uploading"}<LoaderCircle size={14} class="spin" />{:else}<span class="dotq"></span>{/if}
            <span class="mono xs grow ellipsis" title={q.rel}>{q.rel}</span>
            <span class="xs muted">{q.status === "error" ? q.error : q.status === "uploading" ? `${Math.round((q.sent / Math.max(1, q.file.size)) * 100)}%` : q.status === "done" ? bytes(q.file.size) : q.status}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <div
    class="card flush files"
    role="region"
    aria-label="Files"
    class:drag={dragging}
    ondragover={(e) => {
      e.preventDefault();
      dragging = !external;
    }}
    ondragleave={() => (dragging = false)}
    ondrop={drop}
  >
    <div class="tb">
      <nav class="crumbs" aria-label="Folder">
        <button class="crumb" onclick={() => go("")}><Package size={14} />{repo.slug}</button>
        {#each crumbs as c, i}<ChevronRight size={13} class="subtle" /><button class="crumb" onclick={() => go(crumbs.slice(0, i + 1).join("/"))}>{c}</button>{/each}
      </nav>
      <div class="spacer"></div>
      {#if !external}
        <Button size="sm" variant="ghost" icon={FolderPlus} onclick={newFolder}>New folder</Button>
        <Button size="sm" icon={FolderUp} onclick={() => pick(true)}>Upload folder</Button>
        <Button size="sm" variant="primary" icon={Upload} onclick={() => pick(false)}>Upload files</Button>
      {/if}
    </div>
    {#if dragging}<div class="dropzone"><Upload size={22} />Drop files, folders or a .zip to stage them in <b>{path || repo.slug}</b></div>{/if}
    {#if entries === null}
      <div style="padding:16px"><Skeleton lines={5} height={18} /></div>
    {:else if listError}
      <Alert tone="err" title="Could not list this folder">{listError}</Alert>
    {:else}
      <div class="table-wrap">
        <table>
          <thead><tr><th>Name</th><th class="num">Size</th><th class="hide-sm">Modified</th><th></th></tr></thead>
          <tbody>
            {#if path}
              <tr class="clickable" onclick={() => go(crumbs.slice(0, -1).join("/"))}><td colspan="4"><span class="row tight muted"><CornerLeftUp size={14} />Up</span></td></tr>
            {/if}
            {#each rows as r (r.name)}
              <tr class:clickable={r.dir && r.state !== "deleted"} class={r.state} onclick={() => r.dir && r.state !== "deleted" && go(r.full)}>
                <td>
                  <span class="row tight nowrap">
                    {#if r.dir}<Folder size={15} class="ficon dir" />{:else}<FileIcon size={15} class="ficon" />{/if}
                    <span class="nm ellipsis">{r.name}</span>
                    {#if r.addon}<Badge plain tone="info">Addon</Badge>{/if}
                    {#if r.state === "new"}<Badge tone="ok">New</Badge>{:else if r.state === "replaced"}<Badge tone="info">Replaced</Badge>{:else if r.state === "deleted"}<Badge tone="err">Deleted</Badge>{/if}
                  </span>
                </td>
                <td class="num small">{r.dir ? "" : bytes(r.size)}</td>
                <td class="hide-sm small muted">{r.modified ? date(r.modified) : ""}</td>
                <td class="actions">
                  {#if !external}
                    {#if r.state === "deleted"}
                      <Button size="sm" variant="ghost" icon={Undo2} onclick={() => unstage([r.full])}>Restore</Button>
                    {:else if r.virtual}
                      <Button size="sm" variant="ghost" icon={Undo2} onclick={() => unstage(staged.filter((o) => o.path === r.full || o.path.startsWith(r.full + "/")).map((o) => o.path))}>Undo</Button>
                    {:else}
                      <Menu
                        items={[
                          { label: "Rename or move", icon: Pencil, onclick: () => startRename(r) },
                          { label: "Delete", icon: Trash2, danger: true, onclick: () => remove(r) },
                        ]}
                      />
                    {/if}
                  {/if}
                </td>
              </tr>
            {:else}
              <tr><td colspan="4">
                <EmptyState icon={Folder} title="Empty folder" description={external ? undefined : "Drop files or folders here. A dropped .zip can be extracted on the server."} compact>
                  {#if !external}<Button size="sm" variant="primary" icon={Upload} onclick={() => pick(false)}>Upload files</Button>{/if}
                </EmptyState>
              </td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

<Modal open={!!renameRow} title={`Rename ${renameRow?.name ?? ""}`} size="sm" onclose={() => (renameRow = null)}>
  <form class="stack" onsubmit={(e) => (e.preventDefault(), doRename())}>
    <Field label="Name">{#snippet children(id)}<input {id} bind:value={renameName} />{/snippet}</Field>
    <Field label="Folder" hint="Change to move it. Empty = repository root.">{#snippet children(id)}<input {id} class="mono" bind:value={renameDir} placeholder="(root)" />{/snippet}</Field>
  </form>
  {#snippet footer()}
    <Button onclick={() => (renameRow = null)}>Cancel</Button>
    <Button variant="primary" disabled={!renameName.trim() || /[\\/]/.test(renameName)} onclick={doRename}>Stage</Button>
  {/snippet}
</Modal>

<Modal bind:open={reviewOpen} title="Publish a new revision" description={`Revision ${repo.revision + 1} of ${repo.name}`} persistent={publishing}>
  <div class="grid3 rv">
    <div class="card raised"><div class="eyebrow">Uploaded</div><div class="big">{summary.put}</div><div class="xs muted">{bytes(summary.bytes)}</div></div>
    <div class="card raised"><div class="eyebrow">Deleted</div><div class="big">{summary.del}</div></div>
    <div class="card raised"><div class="eyebrow">New folders</div><div class="big">{summary.mkdir}</div></div>
  </div>
  <p class="small muted">The server applies the changes, rebuilds the metadata and switches players to the new revision atomically.</p>
  {#if conflict}
    <Alert tone="warn" title="The repository changed since you started">{conflict}. Someone else published meanwhile; publishing now applies your changes on top of their revision.</Alert>
  {/if}
  {#snippet footer()}
    <Button onclick={() => ((reviewOpen = false), (conflict = null))} disabled={publishing}>Cancel</Button>
    {#if conflict}
      <Button variant="danger-solid" loading={publishing} onclick={() => publish(true)}>Publish anyway</Button>
    {:else}
      <Button variant="primary" icon={Send} loading={publishing} onclick={() => publish(false)}>Publish</Button>
    {/if}
  {/snippet}
</Modal>

<style>
  .staged { display: flex; flex-direction: column; gap: 10px; border-color: var(--accent-line); }
  .sicon { width: 34px; height: 34px; border-radius: var(--r); display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); }
  .ops, .queue { list-style: none; margin: 0; padding: 0; max-height: 220px; overflow: auto; }
  .ops li, .queue li { display: flex; align-items: center; gap: 8px; padding: 3px 0; }
  .ops li.put :global(svg:first-child) { color: var(--ok); }
  .ops li.delete :global(svg:first-child) { color: var(--err); }
  .ops li.delete .mono { text-decoration: line-through; color: var(--fg-2); }
  .ops li.mkdir :global(svg:first-child) { color: var(--info); }
  .queue :global(.spin) { animation: axs-spin 0.9s linear infinite; color: var(--accent); }
  .dotq { width: 14px; height: 14px; border-radius: 50%; border: 1.5px dashed var(--fg-3); }
  .files { position: relative; }
  .files.drag { outline: 2px dashed var(--accent); outline-offset: -4px; }
  .dropzone { position: absolute; inset: 52px 8px 8px; z-index: 5; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; background: color-mix(in srgb, var(--surface-1) 90%, transparent); border-radius: var(--r); color: var(--fg-2); pointer-events: none; }
  .dropzone :global(svg) { color: var(--accent); }
  .tb { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
  .crumbs { display: flex; align-items: center; gap: 2px; flex-wrap: wrap; min-width: 0; }
  .crumb { display: inline-flex; align-items: center; gap: 6px; background: none; border: 0; padding: 4px 6px; border-radius: var(--r-sm); color: var(--fg); font: inherit; font-size: 13.5px; font-weight: 550; cursor: pointer; }
  .crumb:hover { background: var(--surface-3); }
  tr :global(.ficon) { color: var(--fg-3); }
  tr :global(.ficon.dir) { color: var(--accent-text); }
  tr.deleted .nm { text-decoration: line-through; color: var(--fg-3); }
  tr.new .nm { color: var(--ok); }
  .rv .big { font-family: var(--font-display); font-size: 28px; font-weight: 600; line-height: 1.1; }
  .rv .card { padding: 12px 14px; }
</style>
