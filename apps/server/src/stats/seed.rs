//! Synthetic statistics for UI development (debug builds only):
//! `armaxsync-server seed-stats --days 120`.

use rand::{Rng, SeedableRng};

use super::collect::{col, day_of, Acc, Counters};
use super::hll::Hll;
use crate::db::now;
use crate::state::Shared;

const H: i64 = 3_600_000;

pub async fn seed(state: &Shared, days: u32) -> anyhow::Result<()> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let repos: Vec<(i64, Vec<String>, u64)> = state
        .repo_list()
        .iter()
        .map(|rt| {
            let p = rt.published.load();
            let mut addons: Vec<String> = p
                .addons
                .iter()
                .map(|a| a.rsplit('/').next().unwrap_or(a).to_owned())
                .collect();
            if addons.is_empty() {
                addons = [
                    "@CBA_A3",
                    "@ace",
                    "@task_force_radio",
                    "@rhsusaf",
                    "@cup_terrains_core",
                    "@zen",
                ]
                .map(String::from)
                .to_vec();
            }
            (rt.record().id, addons, p.total_size.max(5_000_000_000))
        })
        .collect();
    anyhow::ensure!(!repos.is_empty(), "create a repository first");
    // A stable pool of players, some loyal, some occasional.
    let pool: Vec<(u64, f64)> = (0..400u64)
        .map(|i| {
            (
                rng.gen::<u64>() ^ i,
                if i < 80 {
                    0.8
                } else if i < 200 {
                    0.25
                } else {
                    0.05
                },
            )
        })
        .collect();
    let versions = ["0.1.0", "0.2.0", "0.2.1"];
    let end = now() - now().rem_euclid(H);
    let start = end - days as i64 * 24 * H;
    let mut acc = Acc::default();
    let mut t = start;
    while t < end {
        let dt = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(t).unwrap();
        use chrono::{Datelike, Timelike};
        let hour = dt.hour() as f64;
        let weekend = matches!(dt.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun);
        // Evening peak, quiet nights, busier weekends, slow growth.
        let diurnal = (0.15 + (-(hour - 19.0).powi(2) / 18.0).exp()).min(1.2);
        let growth = 0.6 + 0.4 * ((t - start) as f64 / (end - start) as f64);
        let level = diurnal * growth * if weekend { 1.6 } else { 1.0 };
        let day = day_of(t);
        let elapsed_days = ((t - start) / (24 * H)) as usize;
        for (repo, addons, _) in &repos {
            let reqs = (level * rng.gen_range(150.0..400.0)) as u64;
            let mut c = Counters::default();
            let v = &mut c.0;
            v[col::requests] = reqs;
            v[col::checks] = reqs / 6;
            v[col::bytes_meta] = reqs * 900;
            let downloads = (level * rng.gen_range(5.0..40.0)) as u64;
            v[col::files] = downloads;
            v[col::completed] = downloads + reqs / 4;
            v[col::aborted] = (downloads as f64 * rng.gen_range(0.01..0.08)) as u64;
            v[col::ranges] = reqs / 4;
            v[col::bytes] =
                downloads * rng.gen_range(20_000_000..120_000_000) + v[col::ranges] * 400_000;
            v[col::bytes_sidecar] = v[col::bytes] / 40;
            v[col::xfer_ms] = (v[col::bytes] / rng.gen_range(4_000_000..15_000_000)) * 1000;
            v[col::wait_ms] = if level > 1.0 {
                rng.gen_range(0..20_000)
            } else {
                0
            };
            v[col::s2xx] = downloads + v[col::checks];
            v[col::s206] = v[col::ranges];
            v[col::s404] = reqs / 120;
            v[col::s401] = reqs / 300;
            v[col::s503] = if level > 1.3 { rng.gen_range(0..5) } else { 0 };
            v[col::c_axs] = (reqs as f64 * (0.3 + 0.5 * growth.min(1.0) - 0.3)) as u64;
            v[col::c_a3s] = reqs.saturating_sub(v[col::c_axs]) * 9 / 10;
            v[col::c_browser] = reqs / 50;
            v[col::c_other] =
                reqs.saturating_sub(v[col::c_axs] + v[col::c_a3s] + v[col::c_browser]);
            for (i, a) in addons.iter().enumerate() {
                let weight = 1.0 / (i as f64 + 1.5);
                let e = acc
                    .addons
                    .entry((day.clone(), *repo, a.clone()))
                    .or_default();
                e.0 += (v[col::bytes] as f64 * weight / 2.0) as u64;
                e.1 += (downloads as f64 * weight) as u64;
            }
            for (i, ver) in versions.iter().enumerate() {
                // Newer versions take over as time passes.
                let share = match i {
                    0 => (1.0 - elapsed_days as f64 / days as f64).max(0.05),
                    1 => 0.5,
                    _ => (elapsed_days as f64 / days as f64).powi(2),
                };
                *acc.versions
                    .entry((day.clone(), "armaxsync", ver.to_string()))
                    .or_default() += (v[col::c_axs] as f64 * share / 1.5) as u64;
            }
            *acc.versions
                .entry((day.clone(), "arma3sync", String::new()))
                .or_default() += v[col::c_a3s];
            acc.hours.entry((t / H, *repo)).or_default().add(&c);
        }
        if dt.hour() == 0 {
            let mut all = Hll::default();
            for (repo, ..) in &repos {
                let mut h = Hll::default();
                for (p, chance) in &pool {
                    if rng.gen_bool((chance * growth * if weekend { 1.3 } else { 1.0 }).min(1.0)) {
                        let hv = blake3::hash(&p.to_le_bytes());
                        h.insert(u64::from_le_bytes(hv.as_bytes()[..8].try_into().unwrap()));
                    }
                }
                all.merge(&h);
                acc.players.insert((day.clone(), *repo), h);
            }
            acc.players.insert((day.clone(), 0), all);
        }
        t += H;
    }
    // Revision history: a publish every few days with slowly growing size.
    let mut revs = Vec::new();
    for (repo, _, size) in &repos {
        let mut rev = 1i64;
        let mut sz = *size as f64 * 0.8;
        let mut t = start;
        while t < end {
            sz *= rng.gen_range(0.995..1.02);
            revs.push((
                *repo,
                rev,
                t,
                (sz / 4_000_000.0) as i64,
                sz as i64,
                rng.gen_range(0..3),
                rng.gen_range(0..6),
                rng.gen_range(0..2),
                rng.gen_range(4_000..90_000),
            ));
            rev += 1;
            t += rng.gen_range(2..6) * 24 * H;
        }
    }
    state
        .db
        .call(move |c| {
            super::write(c, &acc)?;
            for (repo, rev, at, files, size, n, u, d, ms) in revs {
                c.execute(
                    "INSERT OR REPLACE INTO revision_history(repo_id, revision, at, kind, files, total_size, new_addons, updated_addons, deleted_addons, duration_ms)
                     VALUES (?1,?2,?3,'publish',?4,?5,?6,?7,?8,?9)",
                    rusqlite::params![repo, rev, at, files, size, n, u, d, ms],
                )?;
            }
            Ok(())
        })
        .await
}
