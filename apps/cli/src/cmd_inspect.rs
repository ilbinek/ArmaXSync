//! `armaxsync inspect` — decode legacy/modern metadata files.

use std::path::PathBuf;

use anyhow::Context;
use clap::Args;

use crate::Ctx;

#[derive(Args)]
pub struct InspectArgs {
    /// File to decode (.a3s/* file, *.a3s.repository, *.zsync, manifest, index.json).
    file: PathBuf,
    /// Show the raw Java object graph instead of the typed view.
    #[arg(long)]
    raw: bool,
}

pub fn run(a: InspectArgs, _ctx: &Ctx) -> anyhow::Result<i32> {
    let data = std::fs::read(&a.file).with_context(|| a.file.display().to_string())?;
    let name = a
        .file
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let value: serde_json::Value = if name.ends_with(".zsync") {
        let m = axs_zsync::ZsyncMeta::parse(&data)?;
        serde_json::json!({
            "kind": "zsync",
            "filename": m.header.filename,
            "mtime": m.header.mtime,
            "block_size": m.header.block_size,
            "length": m.header.length,
            "hash_lengths": m.header.hash_lengths,
            "url": m.header.url,
            "sha1": m.header.sha1,
            "blocks": m.blocks.len(),
        })
    } else if name.ends_with(".json.zst") {
        serde_json::to_value(axs_model::Manifest::from_zst(&data)?)?
    } else if name == "index.json" {
        serde_json::to_value(axs_model::RepoIndex::parse(&data)?)?
    } else if name.ends_with(".a3s.repository") {
        let r = axs_legacy::client::read_repository_file(&data)?;
        serde_json::json!({"kind": "legacy-repository", "value": r})
    } else {
        let g = axs_legacy::parse_gz(&data).context("not a gzipped Java serialization stream")?;
        if a.raw {
            axs_javaser::to_json(&g)
        } else {
            match axs_legacy::A3sFile::from_graph(&g) {
                Ok(f) => serde_json::to_value(f)?,
                Err(_) => axs_javaser::to_json(&g),
            }
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(crate::exit::OK)
}
