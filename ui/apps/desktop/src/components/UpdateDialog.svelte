<script lang="ts">
  import { Modal, Button, Progress, ReleaseNotes, Alert, bytes } from "@axs/kit";
  import { Download, ExternalLink } from "@lucide/svelte";
  import { openUrl } from "../api";
  import { app, installUpdate, canInstallUpdate, saveConfig } from "../store.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();
  const u = $derived(app.update);
  const p = $derived(app.updateProgress);
  const blocked = $derived(canInstallUpdate());

  async function skip() {
    if (!u || !app.state) return;
    await saveConfig({ updates: { ...app.state.config.updates, skipped: u.version } });
    app.update = null;
    open = false;
  }
</script>

{#if u}
  <Modal bind:open title={`ArmaXSync ${u.version}`} description={`You have ${u.current}${u.date ? ` · released ${u.date}` : ""}`} persistent={!!p}>
    <div class="notes card raised"><ReleaseNotes text={u.notes} /></div>
    {#if p}
      {#if p.phase === "install"}
        <Progress indeterminate label="Installing… ArmaXSync restarts automatically" />
      {:else}
        <Progress value={p.downloaded} max={p.total ?? 0} indeterminate={!p.total} label={`Downloading ${bytes(p.downloaded)}${p.total ? ` of ${bytes(p.total)}` : ""}`} />
      {/if}
    {:else if u.kind === "manual"}
      <Alert tone="info">This copy was installed by a package manager or runs in portable mode, so it cannot update itself. Download the new version from the release page.</Alert>
    {:else if u.kind === "dev"}
      <Alert tone="info">Development build: updates are only announced.</Alert>
    {:else if blocked}
      <Alert tone="warn">{blocked}</Alert>
    {/if}
    {#snippet footer()}
      {#if !p}
        <Button variant="ghost" onclick={skip}>Skip this version</Button>
        <div class="spacer"></div>
        <Button icon={ExternalLink} onclick={() => openUrl(u.url)}>Release page</Button>
        {#if u.kind === "auto"}
          <Button variant="primary" icon={Download} disabled={!!blocked} onclick={installUpdate}>Install and restart</Button>
        {/if}
      {/if}
    {/snippet}
  </Modal>
{/if}

<style>
  .notes { max-height: 340px; overflow: auto; }
</style>
