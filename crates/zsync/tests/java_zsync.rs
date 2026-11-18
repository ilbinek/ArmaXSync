//! Byte-for-byte parity with the legacy jazsync maker (needs AXS_JAVA_COMPAT=1
//! and `tools/java-compat/build.sh`).

use std::path::Path;

use axs_zsync::{BlockSummer, ZsyncMeta, LEGACY_BLOCK_SIZE};
use sha1::{Digest, Sha1};

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

fn rust_zsync(path: &Path, url: &str, sha1: &str) -> Vec<u8> {
    let data = std::fs::read(path).unwrap();
    let mtime = std::fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut s = BlockSummer::new(LEGACY_BLOCK_SIZE);
    s.update(&data);
    let name = path.file_name().unwrap().to_string_lossy();
    ZsyncMeta::new(
        &name,
        mtime,
        data.len() as u64,
        url,
        sha1,
        LEGACY_BLOCK_SIZE,
        &s.finish(),
    )
    .to_bytes()
}

#[test]
fn matches_jazsync_maker() {
    if std::env::var("AXS_JAVA_COMPAT").ok().as_deref() != Some("1") {
        eprintln!("skipped (set AXS_JAVA_COMPAT=1)");
        return;
    }
    let classes =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/java-compat/build/classes");
    let dir = std::env::temp_dir().join(format!("axs-zsync-compat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 8191 is excluded: the legacy maker throws IndexOutOfBoundsException for that exact size.
    let sizes = [
        1usize, 100, 8190, 8192, 8193, 16384, 20000, 100_000, 1_000_003, 3_000_000, 12_345_678,
    ];
    for (i, &size) in sizes.iter().enumerate() {
        let src = dir.join(format!("file{i}.pbo"));
        let data = pseudo(size, i as u32 + 1);
        std::fs::write(&src, &data).unwrap();
        let sha1 = hex(&Sha1::digest(&data));
        let url = format!("http://example.com/repo/file{i}.pbo");
        let java_out = dir.join(format!("file{i}.pbo.zsync"));
        let st = std::process::Command::new("java")
            .arg("-cp")
            .arg(&classes)
            .arg("axs.ZsyncMake")
            .arg(&src)
            .arg(&java_out)
            .arg(&url)
            .arg(&sha1)
            .status()
            .unwrap();
        assert!(st.success());
        let java = std::fs::read(&java_out).unwrap();
        let ours = rust_zsync(&src, &url, &sha1);
        if java != ours {
            let jh = String::from_utf8_lossy(
                &java[..java.windows(2).position(|w| w == b"\n\n").unwrap()],
            )
            .into_owned();
            let oh = String::from_utf8_lossy(
                &ours[..ours.windows(2).position(|w| w == b"\n\n").unwrap()],
            )
            .into_owned();
            panic!(
                "size {size}: zsync differs\njava:\n{jh}\nours:\n{oh}\nlen java {} ours {}",
                java.len(),
                ours.len()
            );
        }
        // Our parser reads Java's output.
        let parsed = ZsyncMeta::parse(&java).unwrap();
        assert_eq!(parsed.header.length, size as u64);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
