//! Self-update: release check, owner-only install, verification and staging,
//! against a fake release server. Separate test binary because it sets
//! `ARMAXSYNC_INSTALL_KIND` (process-wide).

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use armaxsync_server::auth::{create_user, Role};
use armaxsync_server::{api, build_state, config::Config, updates};
use axum::routing::get;
use serde_json::{json, Value};
use sha2::Digest;

const BLOB: &[u8] = include_bytes!("fixtures/update/blob.bin");
const SIG: &str = include_str!("fixtures/update/blob.bin.sig");

#[derive(Clone)]
struct Release {
    manifest: Arc<Mutex<Value>>,
    binary: Arc<Mutex<Vec<u8>>>,
}

async fn fake_release_server() -> (String, Release) {
    let rel = Release {
        manifest: Arc::new(Mutex::new(Value::Null)),
        binary: Arc::new(Mutex::new(BLOB.to_vec())),
    };
    let r = rel.clone();
    let app = axum::Router::new()
        .route(
            "/server-latest.json",
            get(move || {
                let m = r.manifest.lock().unwrap().clone();
                async move { axum::Json(m) }
            }),
        )
        .route(
            "/bin",
            get({
                let r = rel.clone();
                move || {
                    let b = r.binary.lock().unwrap().clone();
                    async move { b }
                }
            }),
        );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (base, rel)
}

fn manifest(base: &str, version: &str, bin: &[u8]) -> Value {
    json!({
        "version": version,
        "pub_date": "2026-11-19T00:00:00Z",
        "notes": "### Added\n- things",
        "platforms": {
            updates::TARGET: {
                "url": format!("{base}/bin"),
                "signature": SIG.trim(),
                "sha256": hex::encode(sha2::Sha256::digest(bin)),
                "size": bin.len(),
            }
        }
    })
}

mod hex {
    pub fn encode(b: impl AsRef<[u8]>) -> String {
        b.as_ref().iter().map(|x| format!("{x:02x}")).collect()
    }
}

struct Session {
    base: String,
    http: reqwest::Client,
    cookie: String,
    csrf: String,
}

impl Session {
    async fn login(base: &str, user: &str, pass: &str) -> Session {
        let http = reqwest::Client::new();
        let r = http
            .post(format!("{base}/api/v1/auth/login"))
            .json(&json!({"username": user, "password": pass}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let cookie = r.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let me: Value = r.json().await.unwrap();
        Session {
            base: base.to_owned(),
            http,
            cookie,
            csrf: me["csrf"].as_str().unwrap().to_owned(),
        }
    }

    async fn call(&self, m: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut rb = self
            .http
            .request(m, format!("{}/api/v1{path}", self.base))
            .header("cookie", &self.cookie)
            .header("x-csrf-token", &self.csrf);
        if let Some(b) = body {
            rb = rb.json(&b);
        }
        let r = rb.send().await.unwrap();
        let s = r.status().as_u16();
        (s, r.json().await.unwrap_or(Value::Null))
    }

    async fn wait_job(&self, id: i64) -> Value {
        for _ in 0..200 {
            let (_, j) = self
                .call(reqwest::Method::GET, &format!("/jobs/{id}"), None)
                .await;
            if matches!(
                j["status"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                return j;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("job {id} did not finish");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn self_update_flow() {
    std::env::set_var("ARMAXSYNC_INSTALL_KIND", "systemd");
    let (rel_base, rel) = fake_release_server().await;
    *rel.manifest.lock().unwrap() = manifest(&rel_base, "99.0.0", BLOB);

    let dir = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut cfg = Config {
        data_dir: dir.path().to_path_buf(),
        public_url: format!("http://127.0.0.1:{port}"),
        ..Default::default()
    };
    cfg.updates.manifest_url = format!("{rel_base}/server-latest.json");
    let st = build_state(cfg).await.unwrap();
    create_user(&st, "owner", "correct horse", Role::Owner)
        .await
        .unwrap();
    create_user(&st, "admin", "correct horse", Role::Admin)
        .await
        .unwrap();
    let app = api::router(st.clone());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let base = format!("http://127.0.0.1:{port}");
    use reqwest::Method as M;

    // Every response carries the server version.
    let r = reqwest::get(format!("{base}/healthz")).await.unwrap();
    assert_eq!(r.headers()["x-axs-version"], env!("CARGO_PKG_VERSION"));

    let owner = Session::login(&base, "owner", "correct horse").await;
    let admin = Session::login(&base, "admin", "correct horse").await;

    let (s, sys) = owner.call(M::GET, "/system", None).await;
    assert_eq!(s, 200);
    assert_eq!(sys["update"]["install_kind"], "systemd");
    assert_eq!(sys["update"]["available"], false);

    // Check finds the release.
    let (s, u) = admin.call(M::POST, "/system/update/check", None).await;
    assert_eq!(s, 200, "{u}");
    assert_eq!(u["available"], true);
    assert_eq!(u["can_install"], true);
    assert_eq!(u["latest"]["version"], "99.0.0");
    assert!(u["latest"]["url"]
        .as_str()
        .unwrap()
        .ends_with("/releases/tag/v99.0.0"));

    // Only the owner installs; the version must match what was offered.
    let (s, _) = admin
        .call(
            M::POST,
            "/system/update/install",
            Some(json!({"version": "99.0.0"})),
        )
        .await;
    assert_eq!(s, 403);
    let (s, _) = owner
        .call(
            M::POST,
            "/system/update/install",
            Some(json!({"version": "98.0.0"})),
        )
        .await;
    assert_eq!(s, 409);
    let (s, r) = owner
        .call(
            M::POST,
            "/system/update/install",
            Some(json!({"version": "99.0.0"})),
        )
        .await;
    assert_eq!(s, 200, "{r}");
    let job = owner.wait_job(r["job"].as_i64().unwrap()).await;
    assert_eq!(job["status"], "succeeded", "{job}");

    // Staged for the root helper.
    let ud = dir.path().join("update");
    assert_eq!(
        std::fs::read(ud.join("armaxsync-server.new")).unwrap(),
        BLOB
    );
    assert_eq!(
        std::fs::read_to_string(ud.join("armaxsync-server.new.sig"))
            .unwrap()
            .trim(),
        SIG.trim()
    );
    let req: Value = serde_json::from_slice(&std::fs::read(ud.join("request")).unwrap()).unwrap();
    assert_eq!(req["version"], "99.0.0");
    let (_, sys) = owner.call(M::GET, "/system", None).await;
    assert_eq!(sys["update"]["staged"], "99.0.0");
    let (s, _) = owner
        .call(
            M::POST,
            "/system/update/install",
            Some(json!({"version": "99.0.0"})),
        )
        .await;
    assert_eq!(s, 409, "second install while one is staged");
    // The helper writes its result; the UI reads it back.
    std::fs::remove_file(ud.join("request")).unwrap();
    std::fs::write(
        ud.join("last-result.json"),
        r#"{"status":"installed","version":"99.0.0","message":"ok","at":1}"#,
    )
    .unwrap();
    let (_, sys) = owner.call(M::GET, "/system", None).await;
    assert_eq!(sys["update"]["last_result"]["status"], "installed");

    // A binary that does not match its signature is rejected (checksum matches).
    let tampered = b"hello armaxsync, evil\n".to_vec();
    *rel.binary.lock().unwrap() = tampered.clone();
    *rel.manifest.lock().unwrap() = manifest(&rel_base, "99.0.1", &tampered);
    owner.call(M::POST, "/system/update/check", None).await;
    let (_, r) = owner
        .call(
            M::POST,
            "/system/update/install",
            Some(json!({"version": "99.0.1"})),
        )
        .await;
    let job = owner.wait_job(r["job"].as_i64().unwrap()).await;
    assert_eq!(job["status"], "failed");
    assert!(
        job["error"].as_str().unwrap().contains("signature"),
        "{job}"
    );
    assert!(!ud.join("request").exists());

    // Checksum mismatch.
    let mut m = manifest(&rel_base, "99.0.2", BLOB);
    m["platforms"][updates::TARGET]["sha256"] = json!("00".repeat(32));
    *rel.binary.lock().unwrap() = BLOB.to_vec();
    *rel.manifest.lock().unwrap() = m;
    owner.call(M::POST, "/system/update/check", None).await;
    let (_, r) = owner
        .call(
            M::POST,
            "/system/update/install",
            Some(json!({"version": "99.0.2"})),
        )
        .await;
    let job = owner.wait_job(r["job"].as_i64().unwrap()).await;
    assert_eq!(job["status"], "failed");
    assert!(job["error"].as_str().unwrap().contains("checksum"), "{job}");

    // No newer release → nothing to install.
    *rel.manifest.lock().unwrap() = manifest(&rel_base, "0.0.1", BLOB);
    let (_, u) = owner.call(M::POST, "/system/update/check", None).await;
    assert_eq!(u["available"], false);
    let (s, _) = owner
        .call(
            M::POST,
            "/system/update/install",
            Some(json!({"version": "0.0.1"})),
        )
        .await;
    assert_eq!(s, 409);
}

#[test]
fn verify_update_cli_checks_signature_and_version() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("bin");
    let s = d.path().join("bin.sig");
    std::fs::write(&f, BLOB).unwrap();
    std::fs::write(&s, SIG).unwrap();
    updates::verify_update_cli(&f, &s, "99.0.0").unwrap();
    assert!(
        updates::verify_update_cli(&f, &s, "0.0.1").is_err(),
        "downgrade"
    );
    std::fs::write(&f, b"other").unwrap();
    assert!(
        updates::verify_update_cli(&f, &s, "99.0.0").is_err(),
        "tampered"
    );
}
