//! `armaxsync push` — upload a local folder to an ArmaXSync server repository.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clap::Args;

use axs_client::push::{PushEvent, PushOptions, PushTarget};

use crate::{exit, output, Ctx};

#[derive(Args)]
pub struct PushArgs {
    /// Registered repository whose admin settings hold the push target,
    /// or omit and pass --server/--repo/--token.
    name: Option<String>,
    /// Local folder (default: the repository's admin path).
    #[arg(long)]
    path: Option<PathBuf>,
    /// Server base URL, e.g. https://mods.example.com
    #[arg(long)]
    server: Option<String>,
    /// Repository slug on the server.
    #[arg(long = "server-repo")]
    server_repo: Option<String>,
    #[arg(long, env = "ARMAXSYNC_TOKEN", hide_env_values = true)]
    token: Option<String>,
    /// Only push these repository folders (repeatable), e.g. --scope @ace
    #[arg(long = "scope")]
    scope: Vec<String>,
    /// Delete files on the server (inside the scope) that are not present locally.
    #[arg(long)]
    delete: bool,
    /// Upload but leave the changeset unpublished.
    #[arg(long)]
    no_commit: bool,
    /// Publish even if the repository changed meanwhile.
    #[arg(long)]
    force: bool,
    /// Remember --server/--server-repo/--token/--path for the registered repository.
    #[arg(long)]
    save: bool,
}

pub async fn run(a: PushArgs, ctx: &Ctx) -> anyhow::Result<i32> {
    let mut client = ctx.client()?;
    let mut target = PushTarget {
        server: String::new(),
        repo: String::new(),
        token: String::new(),
    };
    let mut path = a.path.clone();
    if let Some(n) = &a.name {
        let r = client.repo(n)?.clone();
        if let Some(adm) = &r.admin {
            target.server = adm.push_server.clone().unwrap_or_default();
            target.repo = adm.push_repo.clone().unwrap_or_default();
            target.token = adm.push_token.clone().unwrap_or_default();
            if path.is_none() {
                path = adm.path.clone();
            }
        }
        if a.save {
            let mut r2 = r.clone();
            let adm = r2.admin.get_or_insert_with(Default::default);
            if let Some(v) = &a.server {
                adm.push_server = Some(v.clone());
            }
            if let Some(v) = &a.server_repo {
                adm.push_repo = Some(v.clone());
            }
            if let Some(v) = &a.token {
                adm.push_token = Some(v.clone());
            }
            if let Some(v) = &a.path {
                adm.path = Some(v.clone());
            }
            client.save_repo(r2)?;
        }
    }
    if let Some(v) = a.server {
        target.server = v;
    }
    if let Some(v) = a.server_repo {
        target.repo = v;
    }
    if let Some(v) = a.token {
        target.token = v;
    }
    anyhow::ensure!(
        !target.server.is_empty() && !target.repo.is_empty(),
        "push target unknown: pass --server and --server-repo"
    );
    anyhow::ensure!(
        !target.token.is_empty(),
        "an API token is required (--token or ARMAXSYNC_TOKEN)"
    );
    let path = path.ok_or_else(|| anyhow::anyhow!("no local folder: pass --path"))?;
    anyhow::ensure!(path.is_dir(), "{} is not a folder", path.display());

    let pb = Arc::new(Mutex::new(output::bar(ctx.quiet, 0, "hashing")));
    let json = ctx.json;
    let pb2 = pb.clone();
    let events = Arc::new(move |e: PushEvent| {
        let pb = pb2.lock().unwrap();
        match e {
            PushEvent::Hashing { done, total } => {
                pb.set_length(total);
                pb.set_position(done);
            }
            PushEvent::Planned {
                unchanged,
                reused,
                delta,
                full,
                deleted,
                upload_bytes,
            } => {
                pb.set_message("uploading");
                pb.set_length(upload_bytes);
                pb.set_position(0);
                let line = format!("{unchanged} unchanged, {reused} reused, {delta} delta, {full} full upload(s), {deleted} deletion(s)");
                if json {
                    output::json_line(&serde_json::json!({"event":"plan","summary": line}));
                } else {
                    pb.println(line);
                }
            }
            PushEvent::Uploading { path, .. } => tracing::info!("uploading {path}"),
            PushEvent::Uploaded { bytes } => pb.inc(bytes),
            PushEvent::Publishing { job } => {
                pb.finish_and_clear();
                if !json {
                    println!("Publishing (job #{job})…");
                }
            }
            PushEvent::JobLog(l) => {
                if !json {
                    println!("  {l}");
                }
            }
        }
    });
    let opts = PushOptions {
        scope: a.scope,
        delete_missing: a.delete,
        commit: !a.no_commit,
        force: a.force,
    };
    let cache = Some(client.dirs.data.join("push-hashcache.zst"));
    let rep = axs_client::push::push(&target, &path, &opts, cache, events).await?;
    pb.lock().unwrap().finish_and_clear();
    if ctx.json {
        output::json_line(&serde_json::to_value(&rep)?);
    } else {
        println!(
            "Pushed changeset #{}: {} sent for {} ({} delta, {} full, {} reused, {} deleted){}",
            rep.changeset,
            output::bytes(rep.bytes_sent),
            output::bytes(rep.bytes_total),
            rep.delta_files,
            rep.full_files,
            rep.reused,
            rep.deleted,
            rep.revision
                .map(|r| format!(" → revision {r}"))
                .unwrap_or_else(|| " (not published)".into())
        );
    }
    Ok(exit::OK)
}
