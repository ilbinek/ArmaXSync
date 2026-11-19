<script lang="ts">
  import { onMount } from "svelte";
  import { Logo, Button, Field, PasswordInput, Alert } from "@axs/kit";
  import { LogIn, ShieldCheck } from "@lucide/svelte";
  import { get, post, login, ApiError, type Me } from "../api";

  let { onlogin, expired = false }: { onlogin: (m: Me) => void; expired?: boolean } = $props();
  let username = $state("");
  let password = $state("");
  let confirm = $state("");
  let token = $state("");
  let error = $state("");
  let needsSetup = $state(false);
  /** Owner created but sign-in failed: retry only the sign-in. */
  let setupDone = $state(false);
  let busy = $state(false);

  onMount(async () => {
    try {
      needsSetup = (await get("/setup")).needs_setup;
    } catch {
      /* ignore */
    }
  });

  const setup = $derived(needsSetup && !setupDone);
  const mismatch = $derived(setup && confirm.length > 0 && confirm !== password);

  async function submit(e: Event) {
    e.preventDefault();
    error = "";
    if (setup && (password.length < 8 || password !== confirm)) {
      error = password.length < 8 ? "Use at least 8 characters." : "The passwords do not match.";
      return;
    }
    busy = true;
    try {
      if (setup) {
        await post("/setup", { token: token.trim(), username, password });
        setupDone = true;
      }
      onlogin(await login(username, password));
    } catch (err) {
      error = err instanceof ApiError && err.status === 429 ? "Too many failed attempts. Wait a minute and try again." : (err as Error).message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap gridbg">
  <form class="card bracket stack" onsubmit={submit}>
    <Logo size={40} wordmark sub="Server admin" />
    <div>
      <h1>{setup ? "Create the owner" : "Sign in"}</h1>
      <p class="muted small" style="margin-top:6px">
        {#if setup}
          First start: create the owner account with the setup token printed in the server log
          (<code>docker compose logs</code> or <code>journalctl -u armaxsync-server</code>). You can also run <code>armaxsync-server init</code>.
        {:else}
          Administer repositories, uploads and users.
        {/if}
      </p>
    </div>
    {#if expired && !setup}<Alert tone="warn">Your session expired. Sign in again.</Alert>{/if}
    {#if setup}
      <Field label="Setup token" required>{#snippet children(id)}<input {id} bind:value={token} autocomplete="off" class="mono" required />{/snippet}</Field>
    {/if}
    <Field label="Username" required>{#snippet children(id)}<input {id} bind:value={username} autocomplete="username" required />{/snippet}</Field>
    <Field label="Password" required hint={setup ? "At least 8 characters." : undefined}>
      {#snippet children(id)}<PasswordInput {id} bind:value={password} autocomplete={setup ? "new-password" : "current-password"} />{/snippet}
    </Field>
    {#if setup}
      <Field label="Repeat password" required error={mismatch ? "Does not match" : null}>
        {#snippet children(id)}<PasswordInput {id} bind:value={confirm} autocomplete="new-password" invalid={mismatch} />{/snippet}
      </Field>
    {/if}
    {#if error}<Alert tone="err">{error}</Alert>{/if}
    <Button type="submit" variant="primary" icon={setup ? ShieldCheck : LogIn} loading={busy}>{setup ? "Create owner and sign in" : "Sign in"}</Button>
  </form>
</div>

<style>
  .wrap { min-height: 100vh; display: grid; place-items: center; padding: 16px; }
  form { width: min(400px, 100%); padding: 26px; gap: 16px; }
  h1 { font-size: 24px; }
</style>
