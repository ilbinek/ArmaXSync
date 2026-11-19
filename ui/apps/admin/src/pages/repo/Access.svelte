<script lang="ts">
  import { onMount } from "svelte";
  import { Section, Button, Modal, Field, PasswordInput, Switch, Alert, CopyField, EmptyState, Badge, Skeleton, confirmDialog, toast, toastError, errorText } from "@axs/kit";
  import { KeyRound, Plus, Trash2, UsersRound, Lock } from "@lucide/svelte";
  import { get, post, put, del, type RepoDetail } from "../../api";
  import { isAdmin, openJob } from "../../session.svelte";

  let { repo }: { repo: RepoDetail } = $props();
  // svelte-ignore state_referenced_locally
  const slug = repo.slug;

  type Cred = { id: number; login: string; note: string; embed: boolean };
  let creds = $state<Cred[] | null>(null);
  let maintainers = $state<string[]>([]);
  let users = $state<{ username: string; role: string; disabled: boolean }[]>([]);
  let open = $state(false);
  let form = $state({ login: "", password: "", embed: false, note: "" });
  let busy = $state(false);
  let formErr = $state<string | null>(null);
  let created = $state<{ login: string; password: string } | null>(null);
  let savingM = $state<string | null>(null);

  async function load() {
    try {
      const [c, m] = await Promise.all([get<Cred[]>(`/repos/${slug}/credentials`), get<string[]>(`/repos/${slug}/maintainers`)]);
      creds = c;
      maintainers = m;
      if (isAdmin()) users = await get("/users");
    } catch (e) {
      toastError(e);
      creds = [];
    }
  }
  onMount(load);

  const loginErr = $derived(!form.login.trim() ? null : /[:\s]/.test(form.login) ? "No spaces or colons" : form.login.trim().toLowerCase() === "anonymous" ? "Reserved name" : creds?.some((c) => c.login === form.login.trim()) ? "Already exists" : null);

  async function add() {
    if (!form.login.trim() || loginErr) return;
    busy = true;
    formErr = null;
    try {
      const r = await post<{ login: string; password: string; job?: number }>(`/repos/${slug}/credentials`, { ...form, login: form.login.trim(), password: form.password || undefined });
      created = { login: r.login, password: r.password };
      open = false;
      if (r.job) openJob(r.job);
      load();
    } catch (e) {
      formErr = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function remove(c: Cred) {
    if (!(await confirmDialog({ title: `Revoke ${c.login}?`, message: "Players using this login can no longer download the repository.", confirmLabel: "Revoke", danger: true }))) return;
    try {
      await del(`/repos/${slug}/credentials/${c.id}`);
      load();
    } catch (e) {
      toastError(e);
    }
  }

  async function toggleMaintainer(u: string, on: boolean) {
    const next = on ? [...maintainers, u] : maintainers.filter((m) => m !== u);
    savingM = u;
    try {
      await put(`/repos/${slug}/maintainers`, next);
      maintainers = next;
      toast(on ? `${u} can now manage ${repo.name}` : `${u} no longer manages ${repo.name}`);
    } catch (e) {
      toastError(e);
    } finally {
      savingM = null;
    }
  }
</script>

<div class="stack loose narrowish">
  {#if created}
    <Alert tone="ok" title={`Login ${created.login} created`} dismissible ondismiss={() => (created = null)}>
      Copy the password now: it is not shown again.
      <div class="stack tight" style="margin-top:8px">
        <CopyField label="Login" value={created.login} />
        <CopyField label="Password" value={created.password} secret />
      </div>
    </Alert>
  {/if}

  <Section title="Download logins" icon={KeyRound} description="Players of a private repository sign in with one of these (HTTP Basic auth, works with ArmA3Sync too)." flush>
    {#snippet actions()}<Button size="sm" variant="primary" icon={Plus} onclick={() => ((open = true), (form = { login: "", password: "", embed: false, note: "" }), (formErr = null))}>Add login</Button>{/snippet}
    {#if repo.visibility === "public"}
      <div class="pad"><Alert tone="info" icon={Lock}>The repository is public, so logins are not required. Make it private under Settings to require one.</Alert></div>
    {/if}
    {#if !creds}
      <div class="pad"><Skeleton lines={3} /></div>
    {:else if !creds.length}
      <EmptyState icon={KeyRound} title="No logins" compact />
    {:else}
      <table>
        <thead><tr><th>Login</th><th>Note</th><th></th><th></th></tr></thead>
        <tbody>
          {#each creds as c (c.id)}
            <tr>
              <td class="mono">{c.login}</td>
              <td class="muted small">{c.note}</td>
              <td>{#if c.embed}<Badge tone="warn" plain>In auto-config</Badge>{/if}</td>
              <td class="actions"><Button size="sm" variant="ghost" icon={Trash2} onclick={() => remove(c)}>Revoke</Button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </Section>

  {#if isAdmin()}
    <Section title="Maintainers" icon={UsersRound} description="Users who can upload, publish and change this repository's settings. Admins can always.">
      {#if users.filter((u) => u.role === "user").length}
        <div class="stack tight">
          {#each users.filter((u) => u.role === "user") as u (u.username)}
            <Switch checked={maintainers.includes(u.username)} disabled={savingM === u.username || u.disabled} label={u.username} hint={u.disabled ? "Disabled account" : undefined} onchange={(v) => toggleMaintainer(u.username, v)} />
          {/each}
        </div>
      {:else}
        <p class="small muted">No regular users yet. <a href="#/users">Create one</a> with the “user” role first.</p>
      {/if}
    </Section>
  {/if}
</div>

<Modal bind:open title="Add download login" persistent={busy}>
  <form class="stack" onsubmit={(e) => (e.preventDefault(), add())}>
    <Field label="Login" required error={loginErr}>{#snippet children(id)}<input {id} bind:value={form.login} autocomplete="off" data-autofocus />{/snippet}</Field>
    <Field label="Password" hint="Leave empty to generate a strong one.">{#snippet children(id)}<PasswordInput {id} bind:value={form.password} autocomplete="new-password" />{/snippet}</Field>
    <Field label="Note">{#snippet children(id)}<input {id} bind:value={form.note} placeholder="e.g. clan members" />{/snippet}</Field>
    <Switch bind:checked={form.embed} label="Embed in the auto-config" hint="Legacy behaviour: players don't type it, but anyone with the auto-config URL can read it. Only one login can be embedded; this replaces the current one." />
    {#if formErr}<Alert tone="err">{formErr}</Alert>{/if}
  </form>
  {#snippet footer()}
    <Button onclick={() => (open = false)} disabled={busy}>Cancel</Button>
    <Button variant="primary" loading={busy} disabled={!form.login.trim() || !!loginErr} onclick={add}>Add login</Button>
  {/snippet}
</Modal>

<style>
  .narrowish { max-width: 900px; }
  .pad { padding: 12px 16px; }
</style>
