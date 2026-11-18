<script lang="ts">
  import { onMount } from "svelte";
  import { get, post, login, type Me } from "../api";

  let { onlogin }: { onlogin: (m: Me) => void } = $props();
  let username = $state("");
  let password = $state("");
  let token = $state("");
  let error = $state("");
  let needsSetup = $state(false);
  let busy = $state(false);

  onMount(async () => {
    try {
      needsSetup = (await get("/setup")).needs_setup;
    } catch {
      /* ignore */
    }
  });

  async function submit(e: Event) {
    e.preventDefault();
    error = "";
    busy = true;
    try {
      if (needsSetup) await post("/setup", { token, username, password });
      onlogin(await login(username, password));
    } catch (err: any) {
      error = err.message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap">
  <form class="card stack" onsubmit={submit}>
    <h1>ArmaXSync</h1>
    <p class="muted">{needsSetup ? "Create the owner account. The setup token is printed in the server log." : "Sign in to administer repositories."}</p>
    {#if needsSetup}
      <label class="field">Setup token <input bind:value={token} autocomplete="off" required /></label>
    {/if}
    <label class="field">Username <input bind:value={username} autocomplete="username" required /></label>
    <label class="field">Password
      <input type="password" bind:value={password} autocomplete={needsSetup ? "new-password" : "current-password"} minlength={needsSetup ? 8 : undefined} required />
    </label>
    {#if error}<div class="alert err">{error}</div>{/if}
    <button class="btn primary" disabled={busy}>{needsSetup ? "Create owner" : "Sign in"}</button>
  </form>
</div>

<style>
  .wrap { min-height: 100vh; display: grid; place-items: center; padding: 16px; }
  form { width: min(380px, 100%); }
</style>
