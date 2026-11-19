<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Button, DataTable, Badge, Modal, Field, PasswordInput, SegmentedControl, Alert, Menu, ago, date, toast, toastError, errorText, confirmDialog } from "@axs/kit";
  import { UserPlus, KeyRound, Ban, CircleCheck, Trash2, Shield, User, Crown, Users as UsersIcon } from "@lucide/svelte";
  import { get, post, patch, del } from "../api";
  import { session } from "../session.svelte";

  type U = { id: number; username: string; role: "owner" | "admin" | "user"; disabled: boolean; maintains: string[]; created_at: number };
  let users = $state<U[] | null>(null);
  let error = $state<string | null>(null);
  const self = $derived(session.me?.user.user_id);

  async function load() {
    try {
      users = await get<U[]>("/users");
      error = null;
    } catch (e) {
      error = errorText(e);
    }
  }
  onMount(load);

  // New user
  let open = $state(false);
  let form = $state({ username: "", password: "", confirm: "", role: "user" as "user" | "admin" });
  let busy = $state(false);
  let formErr = $state<string | null>(null);
  const pwErr = (pw: string, c: string) => (pw && pw.length < 8 ? "At least 8 characters" : c && c !== pw ? "Passwords do not match" : null);

  async function create() {
    if (!form.username.trim() || form.password.length < 8 || form.password !== form.confirm) return;
    busy = true;
    formErr = null;
    try {
      await post("/users", { username: form.username.trim(), password: form.password, role: form.role });
      toast(`User ${form.username} created`);
      open = false;
      load();
    } catch (e) {
      formErr = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function setRole(u: U, role: "admin" | "user") {
    const ok = await confirmDialog({
      title: `Make ${u.username} ${role === "admin" ? "an admin" : "a regular user"}?`,
      message: role === "admin" ? "Admins manage every repository, all users and the server." : "They keep access only to the repositories they maintain.",
      confirmLabel: "Change role",
    });
    if (!ok) return;
    try {
      await patch(`/users/${u.id}`, { role });
      load();
    } catch (e) {
      toastError(e);
    }
  }
  async function toggle(u: U) {
    if (!u.disabled && !(await confirmDialog({ title: `Disable ${u.username}?`, message: "They are signed out and cannot sign in until enabled again. Their API tokens stop working.", confirmLabel: "Disable", danger: true }))) return;
    try {
      await patch(`/users/${u.id}`, { disabled: !u.disabled });
      load();
    } catch (e) {
      toastError(e);
    }
  }
  async function remove(u: U) {
    if (!(await confirmDialog({ title: `Delete ${u.username}?`, message: "Their API tokens are revoked. This cannot be undone.", confirmLabel: "Delete user", danger: true }))) return;
    try {
      await del(`/users/${u.id}`);
      load();
    } catch (e) {
      toastError(e);
    }
  }

  // Password
  let pwFor = $state<U | null>(null);
  let pw = $state({ a: "", b: "" });
  let pwBusy = $state(false);
  async function setPassword() {
    if (!pwFor || pw.a.length < 8 || pw.a !== pw.b) return;
    pwBusy = true;
    try {
      await patch(`/users/${pwFor.id}`, { password: pw.a });
      toast(`Password for ${pwFor.username} changed`);
      pwFor = null;
    } catch (e) {
      toastError(e);
    } finally {
      pwBusy = false;
    }
  }

  const roleIcon = { owner: Crown, admin: Shield, user: User };
  const columns = [
    { key: "user", label: "User", sort: (u: U) => u.username.toLowerCase() },
    { key: "role", label: "Role", sort: (u: U) => u.role, width: "120px" },
    { key: "maint", label: "Maintains", hideSm: true },
    { key: "created", label: "Created", sort: (u: U) => -u.created_at, width: "130px", hideSm: true },
    { key: "actions", label: "", width: "1%" },
  ];
</script>

<div class="page">
  <PageHeader title="Users" subtitle="Admins manage everything. Users maintain only the repositories assigned to them (under a repository's Access tab).">
    {#snippet actions()}<Button variant="primary" icon={UserPlus} onclick={() => ((open = true), (form = { username: "", password: "", confirm: "", role: "user" }), (formErr = null))}>New user</Button>{/snippet}
  </PageHeader>
  <div class="card flush">
    <DataTable rows={users} {columns} loading={!users} {error} onretry={load} key={(u) => u.id} sortBy="user" search={(u, q) => u.username.toLowerCase().includes(q)} searchPlaceholder="Search users…" emptyIcon={UsersIcon} emptyTitle="No users">
      {#snippet cell(u, c)}
        {#if c.key === "user"}
          <span class="row tight"><span class="strong">{u.username}</span>{#if u.id === self}<Badge plain>You</Badge>{/if}{#if u.disabled}<Badge tone="err">Disabled</Badge>{/if}</span>
        {:else if c.key === "role"}
          {@const Icon = roleIcon[u.role]}
          <Badge plain tone={u.role === "owner" ? "accent" : u.role === "admin" ? "info" : "neutral"} icon={Icon}>{u.role}</Badge>
        {:else if c.key === "maint"}<span class="small muted">{u.role === "user" ? u.maintains.join(", ") || "–" : "all"}</span>
        {:else if c.key === "created"}<span class="small muted" title={date(u.created_at)}>{ago(u.created_at)}</span>
        {:else if c.key === "actions"}
          <Menu
            items={[
              { label: "Change password", icon: KeyRound, onclick: () => ((pwFor = u), (pw = { a: "", b: "" })) },
              ...(u.role !== "owner" && u.id !== self
                ? [
                    u.role === "user" ? { label: "Make admin", icon: Shield, onclick: () => setRole(u, "admin") } : { label: "Make regular user", icon: User, onclick: () => setRole(u, "user") },
                    { label: u.disabled ? "Enable" : "Disable", icon: u.disabled ? CircleCheck : Ban, onclick: () => toggle(u) },
                    { divider: true as const },
                    { label: "Delete", icon: Trash2, danger: true, onclick: () => remove(u) },
                  ]
                : []),
            ]}
          />
        {/if}
      {/snippet}
    </DataTable>
  </div>
</div>

<Modal bind:open title="New user" persistent={busy}>
  <form class="stack" onsubmit={(e) => (e.preventDefault(), create())}>
    <Field label="Username" required>{#snippet children(id)}<input {id} bind:value={form.username} autocomplete="off" data-autofocus />{/snippet}</Field>
    <div class="form-grid">
      <Field label="Password" required hint="At least 8 characters." error={pwErr(form.password, "")}>{#snippet children(id)}<PasswordInput {id} bind:value={form.password} autocomplete="new-password" />{/snippet}</Field>
      <Field label="Repeat password" required error={form.confirm ? pwErr(form.password, form.confirm) : null}>{#snippet children(id)}<PasswordInput {id} bind:value={form.confirm} autocomplete="new-password" />{/snippet}</Field>
    </div>
    <Field label="Role" hint={form.role === "admin" ? "Manages every repository, all users and the server." : "Assign repositories under each repository's Access tab."}>
      {#snippet children()}<SegmentedControl bind:value={form.role} options={[{ value: "user", label: "User", icon: User }, { value: "admin", label: "Admin", icon: Shield }]} />{/snippet}
    </Field>
    {#if formErr}<Alert tone="err">{formErr}</Alert>{/if}
  </form>
  {#snippet footer()}
    <Button onclick={() => (open = false)} disabled={busy}>Cancel</Button>
    <Button variant="primary" loading={busy} disabled={!form.username.trim() || form.password.length < 8 || form.password !== form.confirm} onclick={create}>Create user</Button>
  {/snippet}
</Modal>

<Modal open={!!pwFor} title={`Password for ${pwFor?.username ?? ""}`} size="sm" persistent={pwBusy} onclose={() => (pwFor = null)}>
  <form class="stack" onsubmit={(e) => (e.preventDefault(), setPassword())}>
    <Field label="New password" hint="At least 8 characters." error={pwErr(pw.a, "")}>{#snippet children(id)}<PasswordInput {id} bind:value={pw.a} autocomplete="new-password" />{/snippet}</Field>
    <Field label="Repeat" error={pw.b ? pwErr(pw.a, pw.b) : null}>{#snippet children(id)}<PasswordInput {id} bind:value={pw.b} autocomplete="new-password" />{/snippet}</Field>
  </form>
  {#snippet footer()}
    <Button onclick={() => (pwFor = null)}>Cancel</Button>
    <Button variant="primary" loading={pwBusy} disabled={pw.a.length < 8 || pw.a !== pw.b} onclick={setPassword}>Change password</Button>
  {/snippet}
</Modal>
