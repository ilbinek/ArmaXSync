<script lang="ts">
  import { onMount } from "svelte";
  import { bytes, date, router, navigate, toast, copyText } from "@axs/kit";
  import { get, post, del } from "../api";
  import Files from "./repo/Files.svelte";
  import Events from "./repo/Events.svelte";
  import Settings from "./repo/Settings.svelte";
  import Access from "./repo/Access.svelte";
  import Automation from "./repo/Automation.svelte";
  import JobList from "../components/JobList.svelte";
  import JobDialog from "../components/JobDialog.svelte";

  let { slug, isAdmin }: { slug: string; isAdmin: boolean } = $props();
  let repo = $state<any>(null);
  let error = $state("");
  let jobId = $state<number | null>(null);
  let jobsKey = $state(0);

  const tabs = [
    ["overview", "Overview"],
    ["files", "Files"],
    ["events", "Events"],
    ["settings", "Settings"],
    ["access", "Access"],
    ["automation", "Mirror & schedules"],
    ["jobs", "Jobs"],
  ];
  const tab = $derived(router.query.get("tab") ?? "overview");

  export async function reload() {
    try {
      repo = await get(`/repos/${slug}`);
    } catch (e: any) {
      error = e.message;
    }
  }
  onMount(reload);

  function setTab(t: string) {
    navigate(`/repos/${slug}?tab=${t}`);
  }

  async function runJob(kind: "build" | "mirror") {
    try {
      const r = await post(`/repos/${slug}/${kind}`);
      jobId = r.job;
    } catch (e: any) {
      toast(e.message, "err");
    }
  }

  function onJobDone() {
    reload();
    jobsKey++;
  }

  async function remove() {
    const files = repo.mode === "managed" && confirm("Also delete the repository files from the server?");
    if (!confirm(`Delete repository ${repo.name}? Clients will no longer be able to sync it.`)) return;
    await del(`/repos/${slug}?delete_files=${files}`);
    toast("Repository deleted");
    navigate("/");
  }

  async function copy(text: string) {
    if (await copyText(text)) toast("Copied");
    else toast("Copy failed — select the text and press Ctrl+C", "err");
  }
</script>

{#if error}
  <div class="alert err">{error}</div>
{:else if !repo}
  <div class="empty">Loading…</div>
{:else}
  <div class="row" style="margin-bottom:6px">
    <div>
      <div class="small"><a href="#/">Repositories</a> /</div>
      <h1>{repo.name}</h1>
      <div class="muted small">
        revision {repo.revision} · {repo.files} files · {bytes(repo.size)} · {repo.visibility}
        {#if repo.mode === "external"} · folder <code>{repo.path}</code>{/if}
      </div>
    </div>
    <div class="spacer"></div>
    {#if repo.has_mirror}<button class="btn" onclick={() => runJob("mirror")}>Pull from mirror</button>{/if}
    <button class="btn" onclick={() => runJob("build")}>Rebuild</button>
  </div>

  <div class="tabs">
    {#each tabs as [id, label]}
      <button class:active={tab === id} onclick={() => setTab(id)}>{label}</button>
    {/each}
  </div>

  {#if tab === "overview"}
    <div class="grid2">
      <div class="card stack">
        <h2>Share with players</h2>
        <div class="stack small">
          <div>ArmaXSync (one click): <div class="row"><code>{repo.deep_link}</code><button class="btn small" onclick={() => copy(repo.deep_link)}>Copy</button></div></div>
          <div>ArmA3Sync auto-config URL: <div class="row"><code>{repo.autoconfig_url}</code><button class="btn small" onclick={() => copy(repo.autoconfig_url)}>Copy</button></div></div>
          <div>Repository URL: <div class="row"><code>{repo.url}</code></div></div>
          <div>Public page: <a href={"/repo/" + repo.slug} target="_blank">/repo/{repo.slug}</a></div>
        </div>
      </div>
      <div class="card">
        <h2>Status</h2>
        <div class="kv">
          <span>Revision</span><span>{repo.revision}</span>
          <span>Last build</span><span>{date(repo.build_date)}</span>
          <span>Files</span><span>{repo.files}</span>
          <span>Size</span><span>{bytes(repo.size)}</span>
          <span>Served since start</span><span>{bytes(repo.bytes_served)} ({repo.requests} requests)</span>
          <span>Created</span><span>{date(repo.created_at)}</span>
        </div>
        {#if isAdmin}<hr /><button class="btn danger small" onclick={remove}>Delete repository</button>{/if}
      </div>
    </div>
  {:else if tab === "files"}
    <Files {repo} onpublish={(id: number) => (jobId = id)} />
  {:else if tab === "events"}
    <Events {slug} onjob={(id: number) => (jobId = id)} />
  {:else if tab === "settings"}
    <Settings {repo} onsaved={(job: number | null) => { reload(); if (job) jobId = job; }} />
  {:else if tab === "access"}
    <Access {slug} {isAdmin} onjob={(id: number) => (jobId = id)} />
  {:else if tab === "automation"}
    <Automation {repo} onsaved={reload} onjob={(id: number) => (jobId = id)} />
  {:else if tab === "jobs"}
    {#key jobsKey}<JobList repo={slug} onopen={(id: number) => (jobId = id)} />{/key}
  {/if}
{/if}

{#if jobId}
  <JobDialog id={jobId} onclose={() => (jobId = null)} ondone={onJobDone} />
{/if}
