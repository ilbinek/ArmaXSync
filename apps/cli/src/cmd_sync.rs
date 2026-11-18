//! `armaxsync check | sync | verify`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clap::Args;

use axs_client::{Client, SyncRequest};
use axs_sync::{CancellationToken, EventFn, Op, RepoStatus, SyncEvent};

use crate::{exit, output, Ctx};

#[derive(Args)]
pub struct CheckArgs {
    /// Repositories to check (default: all).
    names: Vec<String>,
    /// Exit with code 10 when any repository has updates.
    #[arg(long)]
    fail_on_update: bool,
}

#[derive(Args)]
pub struct SyncArgs {
    /// Repository name.
    name: String,
    /// Destination folder (default: the repository's destination).
    #[arg(long)]
    dest: Option<PathBuf>,
    /// Only the addons of this event.
    #[arg(long)]
    event: Option<String>,
    /// Also remove extra files at the destination root.
    #[arg(long, conflicts_with = "no_exact")]
    exact: bool,
    #[arg(long)]
    no_exact: bool,
    /// Show the plan without changing anything.
    #[arg(long)]
    dry_run: bool,
    /// Never delete local files.
    #[arg(long)]
    no_delete: bool,
    /// Rehash all local files (ignore the cache).
    #[arg(long)]
    deep: bool,
    /// Remember --dest as the repository destination.
    #[arg(long)]
    save_dest: bool,
}

#[derive(Args)]
pub struct VerifyArgs {
    name: String,
    #[arg(long)]
    dest: Option<PathBuf>,
    /// Download/delete what is needed to repair the destination.
    #[arg(long)]
    repair: bool,
}

pub async fn check(a: CheckArgs, client: &mut Client, ctx: &Ctx) -> anyhow::Result<i32> {
    let names: Vec<String> = if a.names.is_empty() {
        client.repos().iter().map(|r| r.name.clone()).collect()
    } else {
        a.names
    };
    let mut updates = false;
    let mut errors = false;
    for n in names {
        let o = client.check(&n).await?;
        updates |= o.status == RepoStatus::Updated;
        errors |= o.status == RepoStatus::Error;
        if ctx.json {
            output::json_line(&serde_json::to_value(&o)?);
        } else {
            let rev = |v: Option<u32>| v.map(|x| x.to_string()).unwrap_or_else(|| "-".into());
            match &o.error {
                Some(e) => println!("{:<30} ERROR    {e}", o.name),
                None => println!(
                    "{:<30} {:<8} local revision {} / remote {} ({:?})",
                    o.name,
                    format!("{:?}", o.status).to_uppercase(),
                    rev(o.local_revision),
                    rev(o.remote_revision),
                    o.source.map(|s| format!("{s:?}")).unwrap_or_default()
                ),
            }
        }
    }
    Ok(if errors {
        exit::NETWORK
    } else if updates && a.fail_on_update {
        exit::UPDATES_AVAILABLE
    } else {
        exit::OK
    })
}

fn progress_events(ctx: &Ctx) -> (EventFn, Arc<Mutex<indicatif::ProgressBar>>) {
    let pb = Arc::new(Mutex::new(output::bar(ctx.quiet, 0, "download")));
    let json = ctx.json;
    let pb2 = pb.clone();
    let f: EventFn = Arc::new(move |e: SyncEvent| {
        if json {
            match &e {
                SyncEvent::Bytes { .. } => {}
                _ => output::json_line(&serde_json::to_value(&e).unwrap_or_default()),
            }
            return;
        }
        let pb = pb2.lock().unwrap();
        match e {
            SyncEvent::Started { bytes, .. } => pb.set_length(bytes),
            SyncEvent::Bytes { n } => pb.inc(n),
            SyncEvent::FileStarted { path, method, size } => {
                pb.set_message(format!("{method:?}"));
                tracing::info!("{path} ({size} bytes, {method:?})");
            }
            SyncEvent::FileDone { reused, .. } => {
                if reused > 0 {
                    // Delta transfers: count reused bytes as progress too.
                    pb.inc(reused.min(pb.length().unwrap_or(0).saturating_sub(pb.position())));
                }
            }
            SyncEvent::FileFailed { path, error } => pb.println(format!("failed: {path}: {error}")),
            SyncEvent::Deleted { path } => tracing::info!("deleted {path}"),
            SyncEvent::Finished { .. } => pb.finish_and_clear(),
        }
    });
    (f, pb)
}

fn print_plan(o: &axs_client::SyncOutcome, ctx: &Ctx, verbose_ops: bool) {
    let p = &o.plan;
    if ctx.json {
        output::json_line(
            &serde_json::json!({"event": "plan", "repository": o.name, "revision": o.revision, "plan": p}),
        );
        return;
    }
    println!(
        "Repository {} revision {} -> {}: {} file(s) to update ({}), {} to delete",
        o.name,
        o.revision,
        o.destination.display(),
        p.fetch_count,
        output::bytes(p.fetch_bytes),
        p.delete_count
    );
    if verbose_ops {
        for op in &p.ops {
            match op {
                Op::Fetch {
                    path,
                    size,
                    local_size,
                    ..
                } => {
                    println!(
                        "  {} {path} ({})",
                        if local_size.is_some() {
                            "update"
                        } else {
                            "new   "
                        },
                        output::bytes(*size)
                    )
                }
                Op::Delete { path, dir } => {
                    println!("  delete {path}{}", if *dir { "/" } else { "" })
                }
                Op::Mkdir { .. } => {}
            }
        }
    }
}

pub async fn sync(a: SyncArgs, client: &mut Client, ctx: &Ctx) -> anyhow::Result<i32> {
    if a.save_dest {
        if let Some(d) = &a.dest {
            let mut r = client.repo(&a.name)?.clone();
            r.destination = Some(d.clone());
            client.save_repo(r)?;
        }
    }
    let req = SyncRequest {
        destination: a.dest.clone(),
        event: a.event.clone(),
        exact_match: if a.exact {
            Some(true)
        } else if a.no_exact {
            Some(false)
        } else {
            None
        },
        deep: a.deep,
        dry_run: a.dry_run,
        no_delete: a.no_delete,
    };
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("cancelling…");
            c2.cancel();
        }
    });
    let (events, _pb) = progress_events(ctx);
    let out = client.sync(&a.name, &req, events, cancel).await?;
    print_plan(&out, ctx, a.dry_run);
    let Some(rep) = &out.report else {
        return Ok(exit::OK);
    };
    if ctx.json {
        output::json_line(
            &serde_json::json!({"event": "result", "repository": out.name, "report": rep}),
        );
    } else if rep.ok() {
        println!(
            "Synchronization with repository: {} finished. {} downloaded ({} transferred, {} reused), {} deleted.",
            out.name,
            rep.downloaded,
            output::bytes(rep.bytes_transferred),
            output::bytes(rep.bytes_reused),
            rep.deleted
        );
    } else if rep.cancelled {
        println!("Synchronization with repository: {} cancelled.", out.name);
    } else {
        println!(
            "Synchronization with repository: {} finished with errors:",
            out.name
        );
        for (p, e) in &rep.failed {
            println!(" - {p}: {e}");
        }
    }
    Ok(if rep.ok() {
        exit::OK
    } else if rep.cancelled {
        130
    } else {
        exit::ERROR
    })
}

pub async fn verify(a: VerifyArgs, client: &mut Client, ctx: &Ctx) -> anyhow::Result<i32> {
    let req = SyncRequest {
        destination: a.dest.clone(),
        deep: true,
        dry_run: !a.repair,
        ..Default::default()
    };
    let (events, _pb) = progress_events(ctx);
    let out = client
        .sync(&a.name, &req, events, CancellationToken::new())
        .await?;
    print_plan(&out, ctx, true);
    let clean = out.plan.fetch_count == 0 && out.plan.delete_count == 0;
    if let Some(rep) = &out.report {
        if !rep.ok() {
            return Ok(exit::ERROR);
        }
        if !ctx.json {
            println!(
                "Repaired: {} file(s) downloaded, {} deleted.",
                rep.downloaded, rep.deleted
            );
        }
        return Ok(exit::OK);
    }
    if !ctx.json {
        println!(
            "{}",
            if clean {
                "Destination matches the repository."
            } else {
                "Destination differs (run with --repair)."
            }
        );
    }
    Ok(if clean { exit::OK } else { exit::VERIFY_FAILED })
}
