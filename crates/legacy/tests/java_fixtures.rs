//! Compatibility tests against fixtures produced by the original Java classes
//! (`tools/java-compat`, `axs.Gen`). Regenerate with:
//!   tools/java-compat/build.sh && java -cp tools/java-compat/build/classes axs.Gen testdata/legacy

use std::path::{Path, PathBuf};

use axs_legacy::{canonical::canonical_json, gunzip, parse_gz, A3sFile};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/legacy")
}

fn fixtures() -> Vec<(String, Vec<u8>, serde_json::Value)> {
    let mut out = vec![];
    for e in std::fs::read_dir(fixtures_dir()).expect("testdata/legacy") {
        let p = e.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) != Some("a3s") {
            continue;
        }
        let name = p.file_stem().unwrap().to_string_lossy().into_owned();
        let json_text = std::fs::read_to_string(p.with_extension("json")).unwrap();
        out.push((
            name,
            std::fs::read(&p).unwrap(),
            serde_json::from_str(&json_text).unwrap(),
        ));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(out.len() >= 10, "fixtures missing");
    out
}

#[test]
fn reader_matches_java_view() {
    for (name, gz, expected) in fixtures() {
        let g = parse_gz(&gz).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            canonical_json(&g),
            expected,
            "{name}: canonical dump differs"
        );
    }
}

#[test]
fn typed_roundtrip_preserves_semantics_and_bytes() {
    let mut byte_identical = vec![];
    for (name, gz, expected) in fixtures() {
        let g = parse_gz(&gz).unwrap();
        let typed = A3sFile::from_graph(&g).unwrap_or_else(|e| panic!("{name}: {e}"));
        let raw = typed.to_raw();
        let back = axs_javaser::parse(&raw).unwrap_or_else(|e| panic!("{name}: reparse {e}"));
        assert_eq!(
            canonical_json(&back),
            expected,
            "{name}: typed round trip differs"
        );
        if gunzip(&gz).unwrap() == raw {
            byte_identical.push(name.clone());
        } else {
            eprintln!("{name}: not byte identical to Java output");
        }
        assert_eq!(A3sFile::from_graph(&back).unwrap(), typed, "{name}");
    }
    eprintln!("byte identical: {byte_identical:?}");
    // Streams without hash-ordered collections must match Java byte-for-byte.
    for must in [
        "sync_basic",
        "sync_deep",
        "sync_empty",
        "changelogs",
        "autoconfig_http",
        "autoconfig_ftp",
        "serverinfo_empty",
    ] {
        assert!(
            byte_identical.iter().any(|n| n == must),
            "{must} should be byte identical"
        );
    }
}

/// Java reads what Rust writes. Requires `tools/java-compat/build.sh` to have
/// run; enabled with AXS_JAVA_COMPAT=1 (set in the CI java-interop job).
#[test]
fn java_reads_rust_output() {
    if std::env::var("AXS_JAVA_COMPAT").ok().as_deref() != Some("1") {
        eprintln!("skipped (set AXS_JAVA_COMPAT=1)");
        return;
    }
    let classes =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/java-compat/build/classes");
    let tmp = std::env::temp_dir().join(format!("axs-java-compat-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let fx = fixtures();
    let mut files = vec![];
    for (name, gz, _) in &fx {
        let typed = A3sFile::from_graph(&parse_gz(gz).unwrap()).unwrap();
        let f = tmp.join(format!("{name}.a3s"));
        std::fs::write(&f, axs_legacy::gzip(&typed.to_raw())).unwrap();
        files.push(f);
    }
    let out = std::process::Command::new("java")
        .arg("-cp")
        .arg(&classes)
        .arg("axs.Dump")
        .args(&files)
        .output()
        .expect("run java");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.status.success(),
        "java failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for ((name, _, expected), line) in fx.iter().zip(stdout.lines()) {
        let got: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(&got, expected, "{name}: Java view of Rust output differs");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
