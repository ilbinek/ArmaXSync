// Shared desktop state.
import { call, on, type AppStateOut } from "./api";
import { toast } from "@axs/kit";

export const app = $state<{ state: AppStateOut | null; progress: Record<string, any>; checking: boolean }>({
  state: null,
  progress: {},
  checking: false,
});

export async function refresh() {
  app.state = await call<AppStateOut>("app_state");
}

export async function checkAll(silent = false) {
  if (app.checking || !app.state) return;
  app.checking = true;
  try {
    const res: any[] = await call("repo_check_all");
    await refresh();
    const updated = res.filter((r) => r.status === "updated");
    for (const u of updated) {
      const repo = app.state?.repos.find((r) => r.name === u.name);
      if (!repo) continue;
      if (repo.auto && !app.state?.syncing.includes(repo.name) && repo.destination) {
        await startSync(repo.name);
        toast(`${repo.name}: downloading update (auto)`, "info");
      } else if (repo.notify && !silent) {
        toast(`${repo.name} has updates (revision ${u.remote_revision})`, "info", 8000);
      }
    }
  } catch (e: any) {
    if (!silent) toast(String(e), "err");
  } finally {
    app.checking = false;
  }
}

export async function startSync(name: string, params: Record<string, unknown> = {}) {
  await call("sync_start", { name, params });
  await refresh();
}

let started = false;
export function initEvents() {
  if (started) return;
  started = true;
  on<any>("sync-progress", (p) => {
    app.progress[p.repo] = p;
  });
  on<any>("sync-done", async (p) => {
    delete app.progress[p.repo];
    await refresh();
    if (p.ok) {
      const r = p.outcome?.report;
      toast(r ? `${p.repo} synchronized (${r.downloaded} downloaded, ${r.deleted} deleted)` : `${p.repo} is up to date`);
    } else {
      const failed = p.outcome?.report?.failed?.length;
      toast(`${p.repo}: ${p.error ?? (p.outcome?.report?.cancelled ? "cancelled" : `${failed} file(s) failed`)}`, "err", 10000);
    }
  });
}
