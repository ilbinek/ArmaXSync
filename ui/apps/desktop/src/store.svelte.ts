// Shared desktop state: app snapshot, active profile, live sync progress,
// app updates.
import { toast, toastError, navigate } from "@axs/kit";
import { call, on, type AppStateOut, type CheckOutcome, type Profile, type SyncProgress, type UpdateInfo } from "./api";

export const app = $state<{
  state: AppStateOut | null;
  profile: Profile | null;
  progress: Record<string, SyncProgress>;
  checking: boolean;
  /** Repositories being checked individually. */
  checkingRepo: Record<string, boolean>;
  update: UpdateInfo | null;
  updateChecking: boolean;
  updateProgress: { downloaded: number; total: number | null; phase: string } | null;
  /** Server preselected by the Servers page "Join" button. */
  joinServer: string | null;
}>({
  state: null,
  profile: null,
  progress: {},
  checking: false,
  checkingRepo: {},
  update: null,
  updateChecking: false,
  updateProgress: null,
  joinServer: null,
});

export async function refresh() {
  app.state = await call<AppStateOut>("app_state");
}

export async function loadProfile() {
  app.profile = await call<Profile>("profile_get", { name: app.state?.active_profile ?? null });
}

export async function saveProfile(p: Profile, original?: string) {
  await call("profile_save", { profile: p, original: original ?? p.name });
  if (p.name === app.state?.active_profile || original === app.state?.active_profile) app.profile = structuredClone($state.snapshot(p)) as Profile;
}

export async function switchProfile(name: string) {
  await call("profile_activate", { name });
  await refresh();
  await loadProfile();
}

export async function saveConfig(patch: Partial<AppStateOut["config"]>) {
  if (!app.state) return;
  const config = { ...$state.snapshot(app.state.config), ...patch };
  await call("save_config", { config });
  app.state.config = config as AppStateOut["config"];
}

function handleOutcomes(res: CheckOutcome[], silent: boolean) {
  for (const u of res.filter((r) => r.status === "updated")) {
    const repo = app.state?.repos.find((r) => r.name === u.name);
    if (!repo) continue;
    if (repo.auto && !app.state?.syncing.includes(repo.name) && repo.destination) {
      startSync(repo.name)
        .then(() => toast(`${repo.name}: downloading update`, "info"))
        .catch((e) => toastError(e, repo.name));
    } else if (repo.notify && !silent) {
      toast(`${repo.name}: revision ${u.remote_revision} is available`, "info", {
        action: { label: "Open", run: () => navigate(`/repos/${encodeURIComponent(repo.name)}`) },
      });
    }
  }
}

export async function checkAll(silent = false) {
  if (app.checking || !app.state) return;
  app.checking = true;
  try {
    const res = await call<CheckOutcome[]>("repo_check_all");
    await refresh();
    handleOutcomes(res, silent);
    if (!silent) {
      const n = res.filter((r) => r.status === "updated").length;
      const e = res.filter((r) => r.status === "error").length;
      toast(n ? `${n} repositor${n > 1 ? "ies have" : "y has"} updates` : "All repositories are up to date", n ? "info" : "ok", {
        detail: e ? `${e} could not be checked` : undefined,
      });
    }
  } catch (e) {
    if (!silent) toastError(e);
  } finally {
    app.checking = false;
  }
}

export async function checkRepo(name: string) {
  app.checkingRepo[name] = true;
  try {
    const r = await call<CheckOutcome>("repo_check", { name });
    await refresh();
    if (r.status === "error") toast(`${name}: ${r.error}`, "err");
    else if (r.status === "updated") toast(`${name}: revision ${r.remote_revision} is available`, "info");
    else toast(`${name} is up to date`);
  } catch (e) {
    toastError(e, name);
  } finally {
    delete app.checkingRepo[name];
  }
}

export async function startSync(name: string, params: Record<string, unknown> = {}) {
  await call("sync_start", { name, params });
  await refresh();
}

export async function cancelSync(name: string) {
  try {
    await call("sync_cancel", { name });
  } catch (e) {
    toastError(e, name);
  }
}

// ------------------------------------------------------------------ updates --

export async function checkForUpdate(silent = true) {
  if (app.updateChecking) return;
  app.updateChecking = true;
  try {
    const u = await call<UpdateInfo | null>("update_check");
    const skipped = app.state?.config.updates.skipped;
    app.update = u && (!silent || u.version !== skipped) ? u : null;
    if (!silent) toast(u ? `ArmaXSync ${u.version} is available` : "ArmaXSync is up to date", u ? "info" : "ok");
    if (u && silent && app.state?.config.updates.auto_install && u.kind === "auto" && canInstallUpdate() === null) {
      await installUpdate();
    }
  } catch (e) {
    if (!silent) toastError(e, "Update check failed");
  } finally {
    app.updateChecking = false;
  }
}

/** Why an update cannot be installed right now (null = it can). */
export function canInstallUpdate(): string | null {
  if (Object.keys(app.progress).length || app.state?.syncing.length) return "Wait for the running downloads to finish.";
  return null;
}

export async function installUpdate() {
  app.updateProgress = { downloaded: 0, total: null, phase: "download" };
  try {
    await call("update_install");
  } catch (e) {
    app.updateProgress = null;
    toastError(e, "Update failed");
  }
}

// ------------------------------------------------------------------- events --

let started = false;
export function initEvents() {
  if (started) return;
  started = true;
  on<SyncProgress>("sync-progress", (p) => {
    app.progress[p.repo] = p;
  });
  on<any>("sync-done", async (p) => {
    delete app.progress[p.repo];
    await refresh();
    const r = p.outcome?.report;
    if (p.ok) {
      toast(r ? `${p.repo} synchronized` : `${p.repo} is up to date`, "ok", {
        detail: r ? `${r.downloaded} file(s) downloaded, ${r.deleted} deleted` : undefined,
      });
    } else if (r?.cancelled) {
      toast(`${p.repo}: synchronization cancelled`, "warn");
    } else {
      const failed = r?.failed?.length;
      toast(`${p.repo}: ${p.error ?? `${failed} file(s) failed`}`, "err", {
        detail: failed ? r.failed.slice(0, 3).map((f: [string, string]) => `${f[0]}: ${f[1]}`).join("\n") : undefined,
        action: { label: "Open", run: () => navigate(`/repos/${encodeURIComponent(p.repo)}`) },
      });
    }
  });
  on<{ downloaded: number; total: number | null; phase: string }>("update-progress", (p) => {
    app.updateProgress = p;
  });
}
