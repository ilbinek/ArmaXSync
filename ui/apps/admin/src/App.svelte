<script lang="ts">
  import { onMount } from "svelte";
  import { Toasts, router, matchRoute, navigate } from "@axs/kit";
  import { me as fetchMe, logout, type Me } from "./api";
  import Login from "./pages/Login.svelte";
  import Repos from "./pages/Repos.svelte";
  import Repo from "./pages/Repo.svelte";
  import Jobs from "./pages/Jobs.svelte";
  import Users from "./pages/Users.svelte";
  import Tokens from "./pages/Tokens.svelte";
  import Audit from "./pages/Audit.svelte";
  import Stats from "./pages/Stats.svelte";

  let me = $state<Me | null>(null);
  let loading = $state(true);

  onMount(async () => {
    me = await fetchMe().catch(() => null);
    loading = false;
  });

  const isAdmin = $derived(me?.user.role === "owner" || me?.user.role === "admin");
  const nav = $derived([
    { path: "/", label: "Repositories" },
    { path: "/jobs", label: "Jobs" },
    { path: "/stats", label: "Statistics" },
    ...(isAdmin ? [{ path: "/users", label: "Users" }, { path: "/audit", label: "Audit log" }] : []),
    { path: "/tokens", label: "API tokens" },
  ]);
  const repoParams = $derived(matchRoute("/repos/:slug"));

  async function signOut() {
    await logout();
    me = null;
  }
</script>

<Toasts />
{#if loading}
  <div class="empty">Loading…</div>
{:else if !me}
  <Login onlogin={(m) => (me = m)} />
{:else}
  <div class="layout">
    <aside>
      <a class="brand" href="#/">ArmaXSync</a>
      <nav>
        {#each nav as n}
          <a href={"#" + n.path} class:active={n.path === "/" ? router.path === "/" || router.path.startsWith("/repos") : router.path.startsWith(n.path)}>{n.label}</a>
        {/each}
      </nav>
      <div class="spacer"></div>
      <div class="user small">
        <div><b>{me.user.username}</b> <span class="muted">({me.user.role})</span></div>
        <div class="row"><a href="/" target="_blank">Public page</a> · <button class="btn ghost small" onclick={signOut}>Sign out</button></div>
      </div>
    </aside>
    <main>
      {#if repoParams}
        {#key repoParams.slug}<Repo slug={repoParams.slug} {isAdmin} />{/key}
      {:else if router.path === "/jobs"}
        <Jobs />
      {:else if router.path === "/users" && isAdmin}
        <Users self={me.user.user_id} />
      {:else if router.path === "/tokens"}
        <Tokens {isAdmin} />
      {:else if router.path === "/audit" && isAdmin}
        <Audit />
      {:else if router.path === "/stats"}
        <Stats />
      {:else if router.path === "/"}
        <Repos {isAdmin} />
      {:else}
        <div class="empty">Not found. <a href="#/" onclick={() => navigate("/")}>Back</a></div>
      {/if}
    </main>
  </div>
{/if}

<style>
  .layout { display: grid; grid-template-columns: 210px 1fr; min-height: 100vh; }
  aside { background: var(--panel); border-right: 1px solid var(--line); display: flex; flex-direction: column; padding: 14px 10px; gap: 14px; position: sticky; top: 0; height: 100vh; }
  .brand { font-weight: 700; font-size: 17px; color: var(--fg); padding: 4px 8px; }
  nav { display: flex; flex-direction: column; gap: 2px; }
  nav a { padding: 7px 10px; border-radius: 7px; color: var(--fg); }
  nav a:hover { background: var(--panel-2); text-decoration: none; }
  nav a.active { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  .spacer { flex: 1; }
  .user { padding: 8px; border-top: 1px solid var(--line); display: flex; flex-direction: column; gap: 4px; }
  main { padding: 22px; min-width: 0; max-width: 1200px; }
  @media (max-width: 760px) {
    .layout { grid-template-columns: 1fr; }
    aside { position: static; height: auto; flex-direction: row; flex-wrap: wrap; align-items: center; }
    nav { flex-direction: row; flex-wrap: wrap; }
    .spacer, .user { display: none; }
    main { padding: 14px; }
  }
</style>
