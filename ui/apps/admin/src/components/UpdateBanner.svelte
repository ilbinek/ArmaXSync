<script lang="ts">
  import { Alert, Button, Modal, ReleaseNotes, CopyField, toast, toastError, confirmDialog, ago } from "@axs/kit";
  import { ArrowUpCircle, ExternalLink, RefreshCw } from "@lucide/svelte";
  import { get, post, type SystemInfo } from "../api";
  import { session, isOwner, isAdmin, loadSystem, openJob } from "../session.svelte";

  let open = $state(false);
  let checking = $state(false);
  let dismissed = $state<string | null>(null);

  const u = $derived(session.system?.update);
  const show = $derived(isAdmin() && !!u?.available && !!u.latest && dismissed !== u.latest.version);
  const result = $derived(u?.last_result);
  const resultFresh = $derived(!!result && Date.now() - result.at < 24 * 3600_000 && result.status !== "installed");

  async function check() {
    checking = true;
    try {
      await post("/system/update/check");
      await loadSystem();
      if (!session.system?.update?.available) toast("The server is up to date");
    } catch (e) {
      toastError(e, "Update check failed");
    } finally {
      checking = false;
    }
  }

  async function install() {
    if (!u?.latest) return;
    if (!(await confirmDialog({ title: `Update to ${u.latest.version}?`, message: "The server waits for running jobs, installs the update and restarts. Downloads in progress resume automatically.", confirmLabel: "Update and restart" }))) return;
    try {
      const r = await post<{ job: number }>("/system/update/install", { version: u.latest.version });
      open = false;
      openJob(r.job);
      waitForRestart(u.latest.version);
    } catch (e) {
      toastError(e);
    }
  }

  // Poll until the new version answers, then reload.
  function waitForRestart(version: string) {
    session.restarting = version;
    const started = Date.now();
    const t = setInterval(async () => {
      try {
        const s = await get<SystemInfo>("/system");
        if (s.version === version) {
          clearInterval(t);
          location.reload();
        } else if (s.update?.last_result && s.update.last_result.at > started && s.update.last_result.status !== "installed") {
          clearInterval(t);
          session.restarting = null;
          session.system = s;
          toast(`Update failed: ${s.update.last_result.message}`, "err", { ms: 0 });
        }
      } catch {
        /* restarting */
      }
      if (Date.now() - started > 5 * 60_000) {
        clearInterval(t);
        session.restarting = null;
        toast("The server did not come back with the new version. Check its logs.", "err", { ms: 0 });
      }
    }, 3000);
  }
</script>

{#if resultFresh && result && isAdmin()}
  <Alert tone={result.status === "rolled_back" ? "warn" : "err"} title={result.status === "rolled_back" ? `Update to ${result.version} was rolled back` : "Server update failed"} dismissible>
    {result.message} · {ago(result.at)}
  </Alert>
{/if}

{#if show && u?.latest}
  <Alert tone="accent" icon={ArrowUpCircle} title={`ArmaXSync ${u.latest.version} is available`} dismissible ondismiss={() => (dismissed = u.latest!.version)}>
    You run {u.current}.{#if u.staged} Installing {u.staged}…{/if}
    {#snippet actions()}
      <Button size="sm" onclick={() => (open = true)}>What's new</Button>
      {#if u.can_install && isOwner() && !u.staged}<Button size="sm" variant="primary" onclick={install}>Update and restart</Button>{/if}
    {/snippet}
  </Alert>
{/if}

{#if u?.latest}
  <Modal bind:open title={`ArmaXSync ${u.latest.version}`} description={`This server runs ${u.current}${u.latest.pub_date ? ` · released ${u.latest.pub_date.slice(0, 10)}` : ""}`} size="lg">
    <div class="card raised notes"><ReleaseNotes text={u.latest.notes} /></div>
    {#if u.install_kind === "docker"}
      <p class="small muted">This server runs in Docker. Update the container:</p>
      <CopyField value="docker compose pull && docker compose up -d" />
    {:else if u.install_kind === "manual"}
      <p class="small muted">This server cannot update itself (not installed as a systemd package). Download the new binary from the release page and restart the server.</p>
    {:else if !u.latest.has_build}
      <p class="small muted">The release has no build for {u.target}.</p>
    {:else if !isOwner()}
      <p class="small muted">Only the owner can install updates.</p>
    {/if}
    {#snippet footer()}
      <Button variant="ghost" icon={RefreshCw} loading={checking} onclick={check}>Check again</Button>
      <div class="spacer"></div>
      <Button icon={ExternalLink} href={u!.latest!.url} target="_blank">Release page</Button>
      {#if u!.can_install && isOwner() && !u!.staged}<Button variant="primary" icon={ArrowUpCircle} onclick={install}>Update and restart</Button>{/if}
    {/snippet}
  </Modal>
{/if}

<style>
  .notes { max-height: 380px; overflow: auto; }
</style>
