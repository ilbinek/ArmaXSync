// Reactive loader for stats endpoints: refetches when the query changes.
import { errorText } from "@axs/kit";
import { get } from "../../api";

export function loader<T>(path: () => string) {
  const s = $state<{ data: T | null; loading: boolean; error: string | null }>({ data: null, loading: true, error: null });
  $effect(() => {
    const p = path();
    let alive = true;
    s.loading = true;
    s.error = null;
    get<T>(p)
      .then((d) => alive && (s.data = d))
      .catch((e) => alive && (s.error = errorText(e)))
      .finally(() => alive && (s.loading = false));
    return () => {
      alive = false;
    };
  });
  return s;
}

/** Relative change between two numbers (null when there is no baseline). */
export function change(cur: number, prev: number): number | null {
  if (!prev) return cur ? null : 0;
  return (cur - prev) / prev;
}
