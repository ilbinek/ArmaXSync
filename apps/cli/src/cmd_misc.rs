//! events, changelog, extract-keys, check-remote, import-legacy.

use std::path::PathBuf;

use clap::Args;

use axs_client::Client;

use crate::{exit, output, Ctx};

#[derive(Args)]
pub struct ExtractArgs {
    /// Folder to search for *.bikey files.
    src: PathBuf,
    /// Folder to copy the keys into.
    dst: PathBuf,
}

#[derive(Args)]
pub struct NameArg {
    pub name: String,
}

#[derive(Args)]
pub struct ImportArgs {
    /// Legacy ArmA3Sync installation folder (containing resources/ and profiles/).
    path: PathBuf,
    /// Replace repositories that already exist.
    #[arg(long)]
    overwrite: bool,
}

pub fn extract_keys(a: ExtractArgs, ctx: &Ctx) -> anyhow::Result<i32> {
    let keys = axs_client::tools::extract_bikeys(&a.src, &a.dst)?;
    if ctx.json {
        output::json_line(&serde_json::json!({"copied": keys}));
    } else {
        println!(
            "Extraction done: {} key(s) copied to {}.",
            keys.len(),
            a.dst.display()
        );
    }
    Ok(exit::OK)
}

async fn snapshot(client: &Client, name: &str) -> anyhow::Result<axs_sync::RemoteSnapshot> {
    let r = client.repo(name)?;
    let t = client.transport(r)?;
    Ok(axs_sync::fetch_snapshot(&t, r.prefer_modern).await?)
}

pub async fn events(a: NameArg, client: &Client, ctx: &Ctx) -> anyhow::Result<i32> {
    let s = snapshot(client, &a.name).await?;
    if ctx.json {
        output::json_line(&serde_json::to_value(&s.manifest.events)?);
        return Ok(exit::OK);
    }
    if s.manifest.events.is_empty() {
        println!("No events.");
    }
    for e in &s.manifest.events {
        println!(
            "{}{}",
            e.name,
            e.description
                .as_ref()
                .map(|d| format!(" - {d}"))
                .unwrap_or_default()
        );
        for ad in &e.addons {
            println!(
                "    {}{}",
                ad.name,
                if ad.optional { " (optional)" } else { "" }
            );
        }
        for u in &e.userconfig {
            println!(
                "    userconfig/{}{}",
                u.name,
                if u.optional { " (optional)" } else { "" }
            );
        }
    }
    Ok(exit::OK)
}

pub async fn changelog(a: NameArg, client: &Client, ctx: &Ctx) -> anyhow::Result<i32> {
    let s = snapshot(client, &a.name).await?;
    if ctx.json {
        output::json_line(&serde_json::to_value(&s.manifest.changelogs)?);
        return Ok(exit::OK);
    }
    for c in s.manifest.changelogs.iter().rev() {
        println!(
            "Revision {}{}",
            c.revision,
            if c.content_updated {
                ""
            } else {
                " (no content change)"
            }
        );
        for (label, list) in [
            ("new", &c.new_addons),
            ("updated", &c.updated_addons),
            ("deleted", &c.deleted_addons),
        ] {
            if !list.is_empty() {
                println!("  {label}: {}", list.join(", "));
            }
        }
    }
    Ok(exit::OK)
}

pub async fn check_remote(a: NameArg, client: &Client, ctx: &Ctx) -> anyhow::Result<i32> {
    let r = client.check_remote(&a.name).await?;
    if ctx.json {
        output::json_line(&serde_json::to_value(&r)?);
    } else if r.missing.is_empty() {
        println!(
            "Repository {} - repository is synchronized ({} files checked).",
            a.name, r.checked
        );
    } else {
        println!("Repository {} - repository is not synchronized:", a.name);
        for m in &r.missing {
            println!("  File not found on repository: {m}");
        }
    }
    Ok(if r.missing.is_empty() {
        exit::OK
    } else {
        exit::VERIFY_FAILED
    })
}

pub fn import_legacy(a: ImportArgs, client: &mut Client, ctx: &Ctx) -> anyhow::Result<i32> {
    let rep = axs_client::legacy_import::import_all(client, &a.path, a.overwrite)?;
    if ctx.json {
        output::json_line(&serde_json::to_value(&rep)?);
    } else {
        let r = &rep.repositories;
        println!(
            "Imported {} repositor{}: {}",
            r.repositories.len(),
            if r.repositories.len() == 1 {
                "y"
            } else {
                "ies"
            },
            r.repositories.join(", ")
        );
        println!(
            "Imported {} profile(s): {}",
            rep.profiles.len(),
            rep.profiles.join(", ")
        );
        println!(
            "Imported {} favorite server(s), {} external application(s).",
            rep.favorite_servers, rep.external_apps
        );
        for (n, why) in r.skipped.iter().chain(rep.errors.iter()) {
            println!("  skipped {n}: {why}");
        }
    }
    Ok(exit::OK)
}
