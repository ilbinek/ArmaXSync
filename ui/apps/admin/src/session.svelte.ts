// Admin session: signed-in user, server info, running jobs, the job drawer.
import { get, type Me, type SystemInfo, type Job } from "./api";

export const session = $state<{
  me: Me | null;
  system: SystemInfo | null;
  active: Job[];
  /** Job shown in the global job drawer. */
  jobId: number | null;
  /** Bumped when a job finishes, so pages can reload their data. */
  jobsDone: number;
  restarting: string | null;
}>({ me: null, system: null, active: [], jobId: null, jobsDone: 0, restarting: null });

export const isAdmin = () => session.me?.user.role === "owner" || session.me?.user.role === "admin";
export const isOwner = () => session.me?.user.role === "owner";

export async function loadSystem() {
  try {
    session.system = await get<SystemInfo>("/system");
  } catch {
    /* older server or offline */
  }
}

export function openJob(id: number) {
  session.jobId = id;
}

let known = new Set<number>();
/** Poll active jobs (paused while the tab is hidden). */
export async function pollActive() {
  if (typeof document !== "undefined" && document.hidden) return;
  try {
    const jobs = await get<Job[]>("/jobs?status=active&limit=50");
    const ids = new Set(jobs.map((j) => j.id));
    if ([...known].some((id) => !ids.has(id))) session.jobsDone++;
    known = ids;
    session.active = jobs;
  } catch {
    /* ignore */
  }
}
