<script lang="ts">
  import { onMount } from "svelte";
  import { Modal, date, toast } from "@axs/kit";
  import { get, post, patch, del } from "../api";

  let { self }: { self: number } = $props();
  let users = $state<any[]>([]);
  let open = $state(false);
  let form = $state({ username: "", password: "", role: "user" });
  let pwFor = $state<any>(null);
  let newPw = $state("");

  async function load() {
    users = await get("/users");
  }
  onMount(load);

  async function create() {
    try {
      await post("/users", form);
      open = false;
      form = { username: "", password: "", role: "user" };
      load();
    } catch (e: any) {
      toast(e.message, "err");
    }
  }
  async function setRole(u: any, role: string) {
    await patch(`/users/${u.id}`, { role }).catch((e) => toast(e.message, "err"));
    load();
  }
  async function toggle(u: any) {
    await patch(`/users/${u.id}`, { disabled: !u.disabled }).catch((e) => toast(e.message, "err"));
    load();
  }
  async function remove(u: any) {
    if (!confirm(`Delete user ${u.username}?`)) return;
    await del(`/users/${u.id}`).catch((e) => toast(e.message, "err"));
    load();
  }
  async function setPassword() {
    try {
      await patch(`/users/${pwFor.id}`, { password: newPw });
      toast("Password changed");
      pwFor = null;
      newPw = "";
    } catch (e: any) {
      toast(e.message, "err");
    }
  }
</script>

<div class="row" style="margin-bottom:14px">
  <div><h1>Users</h1><div class="muted">Admins manage everything; users can maintain the repositories assigned to them.</div></div>
  <div class="spacer"></div>
  <button class="btn primary" onclick={() => (open = true)}>New user</button>
</div>

<div class="card flush">
  <table>
    <thead><tr><th>User</th><th>Role</th><th class="hide-sm">Maintains</th><th class="hide-sm">Created</th><th></th></tr></thead>
    <tbody>
      {#each users as u}
        <tr>
          <td><b>{u.username}</b>{#if u.disabled} <span class="badge err">disabled</span>{/if}{#if u.id === self} <span class="badge">you</span>{/if}</td>
          <td>
            {#if u.role === "owner" || u.id === self}{u.role}{:else}
              <select value={u.role} onchange={(e) => setRole(u, (e.target as HTMLSelectElement).value)} style="width:auto"><option value="admin">admin</option><option value="user">user</option></select>
            {/if}
          </td>
          <td class="hide-sm small">{u.maintains.join(", ")}</td>
          <td class="hide-sm">{date(u.created_at)}</td>
          <td style="text-align:right">
            <button class="btn ghost small" onclick={() => (pwFor = u)}>Password</button>
            {#if u.role !== "owner" && u.id !== self}
              <button class="btn ghost small" onclick={() => toggle(u)}>{u.disabled ? "Enable" : "Disable"}</button>
              <button class="btn ghost small danger" onclick={() => remove(u)}>Delete</button>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<Modal bind:open title="New user">
  <label class="field">Username <input bind:value={form.username} /></label>
  <label class="field">Password <span class="hint">At least 8 characters.</span><input type="password" bind:value={form.password} /></label>
  <label class="field">Role <select bind:value={form.role}><option value="user">user (repository maintainer)</option><option value="admin">admin</option></select></label>
  {#snippet footer()}<button class="btn primary" onclick={create}>Create</button>{/snippet}
</Modal>

{#if pwFor}
  <Modal open={true} title={`Password for ${pwFor.username}`}>
    <label class="field">New password <input type="password" bind:value={newPw} /></label>
    {#snippet footer()}<button class="btn" onclick={() => (pwFor = null)}>Cancel</button><button class="btn primary" onclick={setPassword}>Change</button>{/snippet}
  </Modal>
{/if}
