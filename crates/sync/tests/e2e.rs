//! Build → serve → sync → modify → delta sync, in modern and legacy modes.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use axs_build::{BuildOptions, FileHashCache};
use axs_sync::*;
use axs_transport::{Endpoint, Transport};

fn pseudo(n: usize, seed: u32) -> Vec<u8> {
    let mut x = seed | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

fn write(root: &Path, rel: &str, data: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, data).unwrap();
}

async fn serve(dir: &Path) -> u16 {
    let app = axum::Router::new().nest_service("/repo", tower_http::services::ServeDir::new(dir));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    port
}

fn build(root: &Path, port: u16, compress: bool) {
    let mut o = BuildOptions::new(root, "E2E");
    o.protocol =
        axs_legacy::urls::protocol_from_url(&format!("http://127.0.0.1:{port}/repo"), None, None);
    o.connections = 3;
    o.compress_pbo = compress;
    o.hidden_paths = vec!["userconfig".into()];
    axs_build::build(
        &o,
        &mut FileHashCache::in_memory(),
        &|_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
}

async fn sync_once(
    port: u16,
    dest: &Path,
    prefer_modern: bool,
    exact: bool,
    segment: u64,
) -> (SyncPlan, ExecReport, u64) {
    let t = Transport::new(
        Endpoint::parse(&format!("http://127.0.0.1:{port}/repo"), None, None).unwrap(),
    )
    .unwrap();
    let snap = fetch_snapshot(&t, prefer_modern).await.unwrap();
    assert_eq!(
        snap.source,
        if prefer_modern {
            SourceKind::Modern
        } else {
            SourceKind::Legacy
        }
    );
    let opts = PlanOptions {
        exact_match: exact,
        case_insensitive: cfg!(windows),
        ..Default::default()
    };
    let local =
        snapshot_local(dest, &snap.manifest, &opts, &mut FileHashCache::in_memory()).unwrap();
    let p = plan(&snap.manifest, &local, &opts);
    let bytes = Arc::new(AtomicU64::new(0));
    let b2 = bytes.clone();
    let events: EventFn = Arc::new(move |e| {
        if let SyncEvent::Bytes { n } = e {
            b2.fetch_add(n, Ordering::Relaxed);
        }
    });
    let eo = ExecOptions {
        connections: 3,
        ranges: snap.ranges_supported,
        compressed_only: snap.manifest.compressed_only,
        segment_threshold: segment,
        ..Default::default()
    };
    let r = execute(&p, &t, dest, &eo, events, CancellationToken::new()).await;
    (p, r, bytes.load(Ordering::Relaxed))
}

fn assert_same(src: &Path, dest: &Path) {
    for e in walk(src) {
        let rel = e.strip_prefix(src).unwrap();
        let s = rel.to_string_lossy();
        if s.starts_with(".a3s")
            || s.starts_with(".axs")
            || s.ends_with(".zsync")
            || s.ends_with(".pbo.zip")
        {
            continue;
        }
        let d = dest.join(rel);
        if e.is_dir() {
            assert!(d.is_dir(), "missing dir {s}");
        } else {
            assert_eq!(
                std::fs::read(&e).unwrap(),
                std::fs::read(&d).unwrap_or_default(),
                "content differs: {s}"
            );
        }
    }
}

fn walk(p: &Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(p).unwrap().flatten() {
        out.push(e.path());
        if e.path().is_dir() {
            out.extend(walk(&e.path()));
        }
    }
    out
}

async fn scenario(prefer_modern: bool) {
    let www = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let src = www.path();
    write(src, "@ace/addons/big.pbo", &pseudo(3_000_000, 1));
    write(src, "@ace/addons/small.pbo", &pseudo(20_000, 2));
    write(src, "@cba/addons/cba.pbo", &pseudo(50_000, 3));
    write(src, "@Mód ü/addons/uni code.pbo", &pseudo(9_000, 4));
    write(src, "userconfig/cfg.hpp", b"cfg");
    write(src, "empty.txt", b"");
    let port = serve(src).await;
    build(src, port, false);

    let (p, r, _) = sync_once(port, dest.path(), prefer_modern, false, 1_000_000).await;
    assert!(r.ok(), "{:?}", r.failed);
    assert_eq!(p.fetch_count, 6);
    assert_same(src, dest.path());

    // Up to date → empty plan.
    let (p, _, _) = sync_once(port, dest.path(), prefer_modern, false, 0).await;
    assert_eq!(p.fetch_count, 0, "{:?}", p.ops);

    // Modify part of the big file, add/remove files, extra local files.
    let mut big = pseudo(3_000_000, 1);
    big[1_500_000..1_500_100].fill(7);
    big.splice(100_000..100_000, pseudo(500, 9));
    write(src, "@ace/addons/big.pbo", &big);
    std::fs::remove_file(src.join("@cba/addons/cba.pbo")).unwrap();
    write(src, "@cba/addons/new.pbo", &pseudo(1000, 5));
    write(dest.path(), "@ace/addons/stray.pbo", b"x");
    write(dest.path(), "userconfig/mine.hpp", b"keep me");
    write(dest.path(), "root-extra.txt", b"keep without exact");
    build(src, port, false);
    let (p, r, bytes) = sync_once(port, dest.path(), prefer_modern, false, 0).await;
    assert!(r.ok(), "{:?}", r.failed);
    assert_eq!(p.fetch_count, 2);
    assert!(bytes < 100_000, "delta transferred {bytes} bytes");
    assert!(r.bytes_reused > 2_800_000);
    assert!(!dest.path().join("@ace/addons/stray.pbo").exists());
    assert!(!dest.path().join("@cba/addons/cba.pbo").exists());
    assert!(dest.path().join("userconfig/mine.hpp").exists());
    assert!(dest.path().join("root-extra.txt").exists());
    assert_same(src, dest.path());

    // Exact match removes root extras.
    let (_, r, _) = sync_once(port, dest.path(), prefer_modern, true, 0).await;
    assert!(r.ok());
    assert!(!dest.path().join("root-extra.txt").exists());

    // Compressed repository + interrupted download resume.
    std::fs::remove_file(dest.path().join("@ace/addons/small.pbo")).unwrap();
    build(src, port, true);
    let big_dest = dest.path().join("@ace/addons/big.pbo");
    std::fs::remove_file(&big_dest).unwrap();
    let full = std::fs::read(src.join("@ace/addons/big.pbo")).unwrap();
    let part = dest.path().join("@ace/addons/big.pbo.axs-part");
    std::fs::write(&part, &full[..1_000_000]).unwrap();
    let sha = axs_build::hash_file(&src.join("@ace/addons/big.pbo"), None)
        .unwrap()
        .sha1;
    std::fs::write(dest.path().join("@ace/addons/big.pbo.axs-part.sha1"), sha).unwrap();
    let (_, r, bytes) = sync_once(port, dest.path(), prefer_modern, false, 0).await;
    assert!(r.ok(), "{:?}", r.failed);
    assert!(
        bytes < full.len() as u64,
        "resume should not refetch everything ({bytes})"
    );
    assert_same(src, dest.path());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn modern_end_to_end() {
    scenario(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn legacy_end_to_end() {
    scenario(false).await;
}

/// A damaged `.pbo.zip` on the server (seen with the legacy Java builder)
/// falls back to downloading the uncompressed file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn broken_zip_falls_back_to_plain_file() {
    let www = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let src = www.path();
    let data = pseudo(300_000, 42);
    write(src, "@m/addons/a.pbo", &data);
    let port = serve(src).await;
    build(src, port, true);
    let zip = src.join("@m/addons/a.pbo.zip");
    assert!(zip.exists());
    // Truncate the archive: still starts like a zip, but is unreadable.
    let z = std::fs::read(&zip).unwrap();
    std::fs::write(&zip, &z[..z.len() / 2]).unwrap();
    for modern in [true, false] {
        let d = dest.path().join(if modern { "m" } else { "l" });
        std::fs::create_dir_all(&d).unwrap();
        let (_, r, _) = sync_once(port, &d, modern, false, 0).await;
        assert!(r.ok(), "{:?}", r.failed);
        assert_eq!(std::fs::read(d.join("@m/addons/a.pbo")).unwrap(), data);
    }
}
