use std::path::Path;
use std::sync::atomic::AtomicBool;

use axs_build::{build, BuildOptions, FileHashCache};
use axs_legacy::urls::protocol_from_url;

fn write(root: &Path, rel: &str, data: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, data).unwrap();
}

fn opts(root: &Path) -> BuildOptions {
    let mut o = BuildOptions::new(root, "Test Repo");
    o.protocol = protocol_from_url("https://mods.example.com/r/test", Some("user"), Some("pw"));
    o.connections = 3;
    o.hidden_paths = vec!["userconfig".into()];
    o
}

fn run(o: &BuildOptions, cache: &mut FileHashCache) -> axs_build::BuildReport {
    build(o, cache, &|_| {}, &AtomicBool::new(false)).unwrap()
}

#[test]
fn full_build_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "@ace/addons/ace_main.pbo", &vec![7u8; 100_000]);
    write(root, "@ace/addons/ace_main.pbo.ace.bisign", b"sig");
    write(root, "@ace/mod.cpp", b"name=\"ACE\";");
    write(root, "@cba/addons/cba.pbo", &vec![1u8; 20_000]);
    write(root, "userconfig/ace/settings.hpp", b"x");
    write(root, "readme.txt", b"");
    std::fs::create_dir_all(root.join("emptydir")).unwrap();
    let o = opts(root);
    let mut cache = FileHashCache::in_memory();

    let r1 = run(&o, &mut cache);
    assert_eq!(r1.revision, 1);
    assert_eq!(r1.files, 6);
    assert_eq!(r1.hashed_files, 6);
    assert_eq!(r1.zsync_written, 6);
    assert!(r1.content_updated);
    assert_eq!(r1.new_addons, vec!["@ace".to_string(), "@cba".to_string()]);
    assert!(root.join("@ace/addons/ace_main.pbo.zsync").exists());

    let sync = axs_legacy::read_sync(&std::fs::read(root.join(".a3s/sync")).unwrap()).unwrap();
    assert_eq!(sync.name, "racine");
    assert_eq!(sync.count_leaves(), 6);
    let names: Vec<_> = sync.children.iter().map(|c| c.name().to_owned()).collect();
    assert_eq!(
        names,
        ["@ace", "@cba", "emptydir", "userconfig", "readme.txt"]
    );
    let mut empty_sha = None;
    sync.walk_leaves(&mut |p, l| {
        if p == "readme.txt" {
            empty_sha = l.sha1.clone();
        }
    });
    assert_eq!(empty_sha.as_deref(), Some("0"));
    let si = axs_legacy::read_server_info(&std::fs::read(root.join(".a3s/serverinfo")).unwrap())
        .unwrap();
    assert_eq!(si.revision, 1);
    assert_eq!(si.number_of_connections, 3);
    assert_eq!(si.hidden_folder_paths, vec!["userconfig".to_string()]);
    let ac =
        axs_legacy::read_autoconfig(&std::fs::read(root.join(".a3s/autoconfig")).unwrap()).unwrap();
    assert_eq!(ac.repository_name.as_deref(), Some("Test Repo"));
    assert_eq!(
        ac.protocole.as_ref().unwrap().login.as_deref(),
        Some("user")
    );
    let idx =
        axs_model::RepoIndex::parse(&std::fs::read(root.join(".axs/index.json")).unwrap()).unwrap();
    assert_eq!(idx.revision, 1);
    let m = axs_model::Manifest::from_zst(
        &std::fs::read(root.join(".axs").join(&idx.manifest)).unwrap(),
    )
    .unwrap();
    assert_eq!(m.files.len(), 6);
    assert_eq!(m.addons, vec!["@ace".to_string(), "@cba".to_string()]);
    assert!(m.dirs.contains(&"emptydir".to_string()));

    // The zsync sidecar is parseable and consistent.
    let z = axs_zsync::ZsyncMeta::parse(
        &std::fs::read(root.join("@ace/addons/ace_main.pbo.zsync")).unwrap(),
    )
    .unwrap();
    assert_eq!(z.header.length, 100_000);
    assert_eq!(
        z.header.url,
        "https://mods.example.com/r/test/@ace/addons/ace_main.pbo"
    );

    // Unchanged rebuild: nothing rehashed, content not updated.
    let r2 = run(&o, &mut cache);
    assert_eq!(r2.revision, 2);
    assert_eq!(r2.hashed_files, 0);
    assert_eq!(r2.zsync_written, 0);
    assert!(!r2.content_updated);

    // Modify an addon file and remove another addon.
    std::thread::sleep(std::time::Duration::from_millis(20));
    write(root, "@ace/addons/ace_main.pbo", &vec![8u8; 100_000]);
    std::fs::remove_dir_all(root.join("@cba")).unwrap();
    let r3 = run(&o, &mut cache);
    assert_eq!(r3.revision, 3);
    assert_eq!(r3.hashed_files, 1);
    assert!(r3.content_updated);
    assert_eq!(r3.updated_addons, vec!["@ace".to_string()]);
    assert_eq!(r3.deleted_addons, vec!["@cba".to_string()]);
    let cl =
        axs_legacy::read_changelogs(&std::fs::read(root.join(".a3s/changelogs")).unwrap()).unwrap();
    assert_eq!(cl.list.len(), 3);
}

#[test]
fn compression_and_sidecar_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "@m/addons/a.pbo", &vec![3u8; 50_000]);
    write(root, "@m/addons/b.txt", b"hello");
    let mut o = opts(root);
    o.compress_pbo = true;
    let mut cache = FileHashCache::in_memory();
    let r = run(&o, &mut cache);
    assert_eq!(r.zips_written, 1);
    let zip = root.join("@m/addons/a.pbo.zip");
    assert!(zip.exists());
    // Zip has one entry named like the file and inflates to the original.
    let mut za = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
    let mut e = za.by_index(0).unwrap();
    assert_eq!(e.name(), "a.pbo");
    let mut out = vec![];
    std::io::Read::read_to_end(&mut e, &mut out).unwrap();
    assert_eq!(out, vec![3u8; 50_000]);
    let sync = axs_legacy::read_sync(&std::fs::read(root.join(".a3s/sync")).unwrap()).unwrap();
    let mut compressed = vec![];
    sync.walk_leaves(&mut |p, l| compressed.push((p.to_owned(), l.compressed, l.compressed_size)));
    assert!(compressed
        .iter()
        .any(|(p, c, s)| p == "@m/addons/a.pbo" && *c && *s > 0));

    // Disable compression and zsync -> sidecars removed.
    o.compress_pbo = false;
    o.zsync = Some(false);
    let r = run(&o, &mut cache);
    assert_eq!(r.sidecars_removed, 3);
    assert!(!zip.exists());
    assert!(!root.join("@m/addons/a.pbo.zsync").exists());
}

#[test]
fn excluded_folders_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "@m/addons/a.pbo", b"pbo");
    write(root, ".sync/ID", b"resilio state");
    write(root, ".sync/Archive/old.bin", b"x");
    write(root, "@m/notes.txt", b"skip me");
    let mut o = opts(root);
    o.excluded_files = vec![".sync".into(), "@m/notes.txt".into()];
    let r = run(&o, &mut FileHashCache::in_memory());
    assert_eq!(r.files, 1);
    assert!(!root.join(".sync/ID.zsync").exists());
    let sync = axs_legacy::read_sync(&std::fs::read(root.join(".a3s/sync")).unwrap()).unwrap();
    assert!(sync.children.iter().all(|c| c.name() != ".sync"));
}
