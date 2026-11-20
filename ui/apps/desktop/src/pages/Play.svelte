<script lang="ts">
  import { Button, Badge, Skeleton, Alert, Topo, copyText, toast, toastError, confirmDialog, navigate } from "@axs/kit";
  import { Crosshair, Server as ServerIcon, Lock, CircleCheck, CircleAlert, TriangleAlert, LoaderCircle, Copy, ChevronDown, Layers, RefreshCw, Plus, Gamepad2 } from "@lucide/svelte";
  import { call, selectedAddons, type Preview, type Profile, type GroupNode } from "../api";
  import { app, saveProfile, startSync, refresh } from "../store.svelte";

  let { onadd }: { onadd: () => void } = $props();

  let preview = $state<Preview | null>(null);
  let starting = $state(false);
  let showCmd = $state(false);
  let launchAfterSync = $state(false);

  const profile = $derived(app.profile);
  const server = $derived(profile?.server ?? "");
  const servers = $derived(app.state?.config.favorite_servers ?? []);
  const outdated = $derived(app.state?.repos.filter((r) => r.state.status === "updated") ?? []);
  const syncing = $derived(Object.keys(app.progress));
  const selected = $derived(profile ? selectedAddons(profile.groups) : []);
  const modsets = $derived(profile?.groups.filter((g) => g.type === "group" && g.children.some((c) => c.type === "addon" && c.selected)).map((g) => g.name) ?? []);

  // Preview follows the profile.
  $effect(() => {
    const p = profile;
    if (!p) return;
    const snap = $state.snapshot(p);
    call<Preview>("launch_preview", { profile: snap, server: snap.server || null })
      .then((r) => (preview = r))
      .catch((e) => toastError(e));
  });

  // Join from the Servers page.
  $effect(() => {
    if (app.joinServer && profile) {
      const s = app.joinServer;
      app.joinServer = null;
      chooseServer(s);
    }
  });

  type Check = { ok: boolean | "warn" | "busy"; label: string; detail?: string; action?: { label: string; run: () => void } };
  const checks = $derived.by((): Check[] => {
    const c: Check[] = [];
    c.push(
      preview?.error
        ? { ok: false, label: "Game executable", detail: preview.error, action: { label: "Settings", run: () => navigate("/settings") } }
        : { ok: !!preview, label: "Game executable", detail: profile?.executable ?? "" },
    );
    if (syncing.length) c.push({ ok: "busy", label: "Downloading", detail: syncing.join(", ") });
    else if (outdated.length)
      c.push({ ok: "warn", label: `${outdated.length} repositor${outdated.length > 1 ? "ies have" : "y has"} updates`, detail: outdated.map((r) => r.name).join(", "), action: { label: "Sync", run: syncAll } });
    else if (app.state?.repos.length) c.push({ ok: true, label: "Repositories up to date" });
    const missing = preview?.check.missing_addons ?? [];
    c.push(
      missing.length
        ? { ok: false, label: `${missing.length} addon${missing.length > 1 ? "s" : ""} not found`, detail: missing.join(", "), action: { label: "Modsets", run: () => navigate("/modsets") } }
        : { ok: selected.length > 0 ? true : "warn", label: selected.length ? `${selected.length} addons found` : "No addons selected", action: selected.length ? undefined : { label: "Modsets", run: () => navigate("/modsets") } },
    );
    if (preview?.check.duplicate_selection.length) c.push({ ok: "warn", label: "Selected more than once", detail: preview.check.duplicate_selection.join(", ") });
    if (preview?.running) c.push({ ok: "warn", label: "Arma 3 is already running" });
    return c;
  });
  const problems = $derived(checks.filter((c) => c.ok !== true));
  const blocking = $derived(!!preview?.error);

  async function chooseServer(name: string) {
    if (!profile) return;
    const before = $state.snapshot(profile) as Profile;
    const p = structuredClone(before);
    p.server = name || null;
    const fav = servers.find((s) => s.name === name);
    let switched: string | null = null;
    if (fav?.modset) {
      const target = p.groups.find((g) => g.type === "group" && g.name.toLowerCase() === fav.modset!.toLowerCase());
      if (target) {
        const setAll = (n: GroupNode, v: boolean): void => (n.type === "group" ? n.children.forEach((c) => setAll(c, v)) : void (n.selected = v && !n.optional));
        p.groups.forEach((g) => setAll(g, false));
        setAll(target, true);
        switched = target.name;
      }
    }
    try {
      await saveProfile(p);
      if (switched) toast(`Modset switched to ${switched}`, "info", { action: { label: "Undo", run: () => saveProfile(before) } });
    } catch (e) {
      toastError(e);
    }
  }

  async function syncAll() {
    for (const r of outdated) {
      if (!r.destination || app.state?.syncing.includes(r.name)) continue;
      try {
        await startSync(r.name);
      } catch (e) {
        toastError(e, r.name);
      }
    }
  }

  async function syncAndPlay() {
    launchAfterSync = true;
    await syncAll();
  }
  $effect(() => {
    if (launchAfterSync && syncing.length === 0 && outdated.length === 0) {
      launchAfterSync = false;
      launch(true);
    }
  });

  async function launch(skipConfirm = false) {
    if (!profile || blocking) return;
    if (!skipConfirm && problems.length) {
      const ok = await confirmDialog({
        title: "Launch anyway?",
        message: "Some checks did not pass:",
        details: problems.map((p) => (p.detail ? `${p.label}: ${p.detail}` : p.label)),
        confirmLabel: "Launch anyway",
      });
      if (!ok) return;
    }
    starting = true;
    try {
      await call<number>("launch_game", { profile: $state.snapshot(profile), server: profile.server || null });
      toast("Arma 3 is starting", "ok", { detail: server ? `Joining ${server}` : undefined });
    } catch (e) {
      toastError(e, "Could not start Arma 3");
    } finally {
      starting = false;
      refresh();
    }
  }

  const cmdline = $derived(preview ? [preview.command?.program ?? "<executable>", ...(preview.command?.args ?? preview.args)].map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" ") : "");
</script>

{#if !profile || !app.state}
  <div class="page"><Skeleton block height={220} /><Skeleton lines={4} /></div>
{:else}
  <div class="page">
    <section class="hero card bracket">
      <Topo opacity={0.16} coords={`GRID ${String(selected.length).padStart(3, "0")} ${String(app.state.repos.length * 37 + 104).padStart(3, "0")}`} />
      <div class="hero-main">
        <div class="eyebrow">Profile · {profile.name}</div>
        <h1 class="title">
          {#if launchAfterSync}Updating, then launching…{:else if blocking}Set up the game first{:else if problems.length}Almost ready{:else}Ready to deploy{/if}
        </h1>
        <div class="chips">
          <span class="chip"><Layers size={13} />{modsets.length ? modsets.join(" + ") : "No modset"}</span>
          <span class="chip">{selected.length} addons</span>
          <span class="chip"><ServerIcon size={13} />{server || "Main menu"}</span>
        </div>
      </div>
      <div class="launch">
        <Button variant="primary" size="lg" icon={Crosshair} loading={starting} disabled={blocking || launchAfterSync} onclick={() => launch()}>Launch</Button>
        {#if outdated.length && !syncing.length && !launchAfterSync}
          <Button size="sm" variant="subtle" icon={RefreshCw} onclick={syncAndPlay}>Sync & launch</Button>
        {:else if launchAfterSync}
          <Button size="sm" variant="ghost" onclick={() => (launchAfterSync = false)}>Don't launch</Button>
        {/if}
      </div>
    </section>

    {#if !app.state.repos.length}
      <Alert tone="accent" title="No repositories yet">
        Add your community's repository to download its mods.
        {#snippet actions()}<Button size="sm" icon={Plus} onclick={onadd}>Add repository</Button>{/snippet}
      </Alert>
    {/if}

    <div class="cols">
      <section class="stack">
        <div class="row between"><h2 style="margin:0">Server</h2><a href="#/servers" class="small">Manage servers</a></div>
        <div class="servers" role="radiogroup" aria-label="Server to join">
          <button class="srv card interactive" class:selected={!server} role="radio" aria-checked={!server} onclick={() => chooseServer("")}>
            <span class="srv-ic"><Gamepad2 size={18} /></span>
            <span class="grow"><span class="strong">Main menu</span><span class="xs muted block">Start without joining a server</span></span>
          </button>
          {#each servers as s (s.name)}
            <button class="srv card interactive" class:selected={server === s.name} role="radio" aria-checked={server === s.name} onclick={() => chooseServer(s.name)}>
              <span class="srv-ic"><ServerIcon size={18} /></span>
              <span class="grow ellipsis">
                <span class="strong">{s.name}</span>
                <span class="xs muted block mono">{s.host}:{s.port}</span>
              </span>
              {#if s.password}<Lock size={14} class="subtle" />{/if}
              {#if s.modset}<Badge tone="info" plain>{s.modset}</Badge>{/if}
            </button>
          {/each}
        </div>
      </section>

      <section class="stack">
        <h2 style="margin:0">Readiness</h2>
        <ul class="checks card">
          {#if !preview}
            <li><Skeleton lines={3} /></li>
          {:else}
            {#each checks as c}
              <li>
                <span class="ci {c.ok === true ? 'ok' : c.ok === 'warn' ? 'warn' : c.ok === 'busy' ? 'busy' : 'err'}">
                  {#if c.ok === true}<CircleCheck size={18} />{:else if c.ok === "warn"}<TriangleAlert size={18} />{:else if c.ok === "busy"}<LoaderCircle size={18} class="spin" />{:else}<CircleAlert size={18} />{/if}
                </span>
                <span class="grow">
                  <span class="strong small">{c.label}</span>
                  {#if c.detail}<span class="xs muted block det" title={c.detail}>{c.detail}</span>{/if}
                </span>
                {#if c.action}<Button size="sm" onclick={c.action.run}>{c.action.label}</Button>{/if}
              </li>
            {/each}
          {/if}
        </ul>
      </section>
    </div>

    <section class="card stack tight">
      <div class="row between">
        <h2 style="margin:0">Loading {selected.length} addon{selected.length === 1 ? "" : "s"}</h2>
        <a href="#/modsets" class="small">Edit modsets</a>
      </div>
      {#if selected.length}
        <div class="chips">
          {#each selected as a}
            <span class="chip mono" class:missing={preview?.check.missing_addons.includes(a)} title={preview?.check.missing_addons.includes(a) ? "Not found in your addon folders" : undefined}>{a}</span>
          {/each}
        </div>
      {:else}
        <p class="muted small">Vanilla Arma 3: no addons selected.</p>
      {/if}
    </section>

    <section class="card flush">
      <button class="adv" onclick={() => (showCmd = !showCmd)} aria-expanded={showCmd}>
        <ChevronDown size={15} style={showCmd ? "" : "transform:rotate(-90deg)"} />
        <span class="grow">Launch parameters</span>
        <span class="xs subtle">{preview?.args.length ?? 0} arguments</span>
      </button>
      {#if showCmd}
        <div class="adv-body stack tight">
          <div class="log">{cmdline || "…"}</div>
          <div class="row end">
            <a href="#/settings" class="small">Game options</a>
            <Button size="sm" icon={Copy} onclick={async () => (await copyText(cmdline)) && toast("Command line copied")}>Copy</Button>
          </div>
        </div>
      {/if}
    </section>
  </div>
{/if}

<style>
  .hero {
    position: relative; overflow: hidden;
    display: flex; gap: 24px; align-items: center; flex-wrap: wrap; padding: 28px 28px;
    background-color: var(--surface-1);
    background-image: radial-gradient(ellipse at 100% 0%, var(--accent-soft), transparent 60%);
  }
  .hero > :global(:not(.topo)) { position: relative; z-index: 1; }
  .hero-main { flex: 1; min-width: 260px; display: flex; flex-direction: column; gap: 8px; }
  .title { font-size: 38px; line-height: 1; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px; }
  .chip :global(svg) { color: var(--accent-text); }
  .launch { display: flex; flex-direction: column; gap: 8px; align-items: stretch; }
  .launch :global(.btn.lg) { height: 56px; padding: 0 40px; font-size: 20px; letter-spacing: 0.14em; }

  .cols { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 20px; }
  @media (max-width: 1000px) { .cols { grid-template-columns: 1fr; } }
  .servers { display: flex; flex-direction: column; gap: 8px; }
  .srv { display: flex; align-items: center; gap: 12px; padding: 11px 14px; text-align: left; font: inherit; color: var(--fg); width: 100%; }
  .srv-ic { width: 34px; height: 34px; border-radius: var(--r); display: grid; place-items: center; background: var(--surface-3); color: var(--fg-2); flex-shrink: 0; }
  .srv.selected .srv-ic { background: var(--accent); color: var(--accent-fg); }
  .block { display: block; }

  .checks { list-style: none; margin: 0; padding: 6px 14px; }
  .checks li { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--line); }
  .checks li:last-child { border-bottom: 0; }
  .ci { display: grid; place-items: center; }
  .ci.ok { color: var(--ok); }
  .ci.warn { color: var(--warn); }
  .ci.err { color: var(--err); }
  .ci.busy { color: var(--accent); }
  .ci :global(.spin) { animation: axs-spin 0.9s linear infinite; }
  .det { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  .checks .grow { min-width: 0; }

  .adv { display: flex; align-items: center; gap: 8px; width: 100%; padding: 12px 16px; background: none; border: 0; color: var(--fg); font: inherit; cursor: pointer; text-align: left;
    font-family: var(--font-display); font-size: 15px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; }
  .adv:hover { background: var(--surface-2); }
  .adv-body { padding: 0 16px 14px; }
  .chip.mono { font-family: var(--mono); font-size: 12px; }
  .chip.missing { color: var(--err); border-color: var(--err); background: var(--err-soft); }
</style>
