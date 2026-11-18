//! In-process end-to-end tests of the server.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use armaxsync_server::auth::{create_user, Role};
use armaxsync_server::{api, build_state, config::Config};
use axs_sync::{CancellationToken, EventFn, ExecOptions, PlanOptions, SourceKind};
use axs_transport::{Endpoint, Transport};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

struct Srv {
    base: String,
    port: u16,
    http: reqwest::Client,
    token: String,
    _dir: tempfile::TempDir,
}

impl Srv {
    async fn start() -> Srv {
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let cfg = Config {
            data_dir: dir.path().to_path_buf(),
            public_url: format!("http://127.0.0.1:{port}"),
            job_workers: 2,
            ..Default::default()
        };
        let st = build_state(cfg).await.unwrap();
        create_user(&st, "owner", "correct horse", Role::Owner)
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
        let http = reqwest::Client::new();
        // Login → session cookie + CSRF → API token.
        let r = http
            .post(format!("{base}/api/v1/auth/login"))
            .json(&json!({"username":"owner","password":"correct horse"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let cookie = r
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let me: Value = r.json().await.unwrap();
        let csrf = me["csrf"].as_str().unwrap().to_owned();
        // Missing CSRF header is rejected.
        let r = http
            .post(format!("{base}/api/v1/tokens"))
            .header("cookie", &cookie)
            .json(&json!({"name":"t","scopes":["admin"]}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 403);
        let r: Value = http
            .post(format!("{base}/api/v1/tokens"))
            .header("cookie", &cookie)
            .header("x-csrf-token", &csrf)
            .json(&json!({"name":"tests","scopes":["admin"]}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let token = r["token"].as_str().unwrap().to_owned();
        Srv {
            base,
            port,
            http,
            token,
            _dir: dir,
        }
    }

    fn req(&self, m: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(m, format!("{}/api/v1{path}", self.base))
            .bearer_auth(&self.token)
    }

    async fn json(&self, m: reqwest::Method, path: &str, body: Value) -> Value {
        let r = self.req(m, path).json(&body).send().await.unwrap();
        let st = r.status();
        let v: Value = r.json().await.unwrap_or(Value::Null);
        assert!(st.is_success(), "{path}: {st} {v}");
        v
    }

    async fn wait_job(&self, id: i64) -> Value {
        for _ in 0..600 {
            let j: Value = self
                .req(reqwest::Method::GET, &format!("/jobs/{id}"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            match j["status"].as_str().unwrap() {
                "queued" | "running" => tokio::time::sleep(Duration::from_millis(50)).await,
                "succeeded" => return j,
                other => panic!("job {id} {other}: {}", j),
            }
        }
        panic!("job {id} timed out");
    }

    async fn publish(&self, slug: &str, files: &[(&str, Vec<u8>)], deletes: &[&str]) -> Value {
        let cs = self
            .json(
                reqwest::Method::POST,
                &format!("/repos/{slug}/changesets"),
                json!({}),
            )
            .await["id"]
            .as_i64()
            .unwrap();
        for (p, data) in files {
            let enc: String = p
                .split('/')
                .map(|s| {
                    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC)
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("/");
            let r = self
                .req(
                    reqwest::Method::PUT,
                    &format!("/changesets/{cs}/files/{enc}"),
                )
                .body(data.clone())
                .send()
                .await
                .unwrap();
            assert!(r.status().is_success(), "{p}: {}", r.text().await.unwrap());
        }
        if !deletes.is_empty() {
            let ops: Vec<Value> = deletes
                .iter()
                .map(|d| json!({"op":"delete","path": d}))
                .collect();
            self.json(
                reqwest::Method::POST,
                &format!("/changesets/{cs}/ops"),
                json!(ops),
            )
            .await;
        }
        let j = self
            .json(
                reqwest::Method::POST,
                &format!("/changesets/{cs}/commit"),
                json!({}),
            )
            .await["job"]
            .as_i64()
            .unwrap();
        self.wait_job(j).await
    }

    fn transport(&self, slug: &str, login: Option<(&str, &str)>) -> Transport {
        let ep = Endpoint::parse(
            &format!("{}/r/{slug}", self.base),
            login.map(|l| l.0),
            login.map(|l| l.1),
        )
        .unwrap();
        Transport::new(ep).unwrap()
    }
}

async fn client_sync(
    t: &Transport,
    dest: &Path,
    modern: bool,
) -> (axs_sync::SyncPlan, axs_sync::ExecReport, u64) {
    let snap = axs_sync::fetch_snapshot(t, modern).await.unwrap();
    assert_eq!(
        snap.source,
        if modern {
            SourceKind::Modern
        } else {
            SourceKind::Legacy
        }
    );
    let po = PlanOptions::default();
    let local = axs_sync::snapshot_local(
        dest,
        &snap.manifest,
        &po,
        &mut axs_build::FileHashCache::in_memory(),
    )
    .unwrap();
    let plan = axs_sync::plan(&snap.manifest, &local, &po);
    let bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let b2 = bytes.clone();
    let ev: EventFn = Arc::new(move |e| {
        if let axs_sync::SyncEvent::Bytes { n } = e {
            b2.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
        }
    });
    let eo = ExecOptions {
        connections: 3,
        ranges: snap.ranges_supported,
        ..Default::default()
    };
    let rep = axs_sync::execute(&plan, t, dest, &eo, ev, CancellationToken::new()).await;
    (plan, rep, bytes.load(std::sync::atomic::Ordering::Relaxed))
}

async fn raw_get(port: u16, path_bytes: &[u8], extra: &str) -> String {
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let mut req = b"GET ".to_vec();
    req.extend_from_slice(path_bytes);
    req.extend_from_slice(
        format!(" HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{extra}\r\n").as_bytes(),
    );
    s.write_all(&req).await.unwrap();
    let mut out = vec![];
    s.read_to_end(&mut out).await.unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_end_to_end() {
    let srv = Srv::start().await;
    // Create and publish.
    let r = srv
        .json(
            reqwest::Method::POST,
            "/repos",
            json!({"slug":"main","name":"Main Repo","description":"test"}),
        )
        .await;
    srv.wait_job(r["job"].as_i64().unwrap()).await;
    let big = pseudo(3_000_000, 1);
    let j = srv
        .publish(
            "main",
            &[
                ("@ace/addons/ace_main.pbo", big.clone()),
                ("@ace/mod.cpp", b"name=\"ACE\";".to_vec()),
                ("@Mód ü/addons/uni code.pbo", pseudo(9000, 2)),
                ("userconfig/ace/settings.hpp", b"cfg".to_vec()),
                ("empty.txt", vec![]),
            ],
            &[],
        )
        .await;
    assert_eq!(j["summary"]["revision"], 2);
    let detail = srv
        .json(reqwest::Method::GET, "/repos/main", Value::Null)
        .await;
    assert_eq!(detail["files"], 5);

    // Modern + legacy client syncs.
    let t = srv.transport("main", None);
    for modern in [true, false] {
        let dest = tempfile::tempdir().unwrap();
        let (plan, rep, _) = client_sync(&t, dest.path(), modern).await;
        assert!(rep.ok(), "{:?}", rep.failed);
        assert_eq!(plan.fetch_count, 5);
        assert_eq!(
            std::fs::read(dest.path().join("@ace/addons/ace_main.pbo")).unwrap(),
            big
        );
        assert!(dest.path().join("@Mód ü/addons/uni code.pbo").exists());
    }
    // Legacy autoconfig points at the public URL.
    let ac = axs_legacy::read_autoconfig(&t.get_bytes(".a3s/autoconfig").await.unwrap()).unwrap();
    assert_eq!(ac.repository_name.as_deref(), Some("Main Repo"));
    assert_eq!(
        ac.protocole.unwrap().url.as_deref(),
        Some("127.0.0.1/r/main")
    );

    // Raw HTTP behaviour legacy clients rely on.
    let resp = raw_get(srv.port, "/r/main//empty.txt".as_bytes(), "").await;
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
    let mut p = b"/r/main/@M".to_vec();
    p.extend_from_slice("ód%20ü/addons/uni%20code.pbo".as_bytes());
    let resp = raw_get(srv.port, &p, "Range: bytes=8000-9500\r\n").await;
    assert!(resp.starts_with("HTTP/1.1 206"), "{resp}");
    assert!(
        resp.to_lowercase()
            .contains("content-range: bytes 8000-8999/9000"),
        "{resp}"
    );
    assert!(
        resp.to_lowercase().contains("content-length: 1000"),
        "{resp}"
    );
    let resp = raw_get(srv.port, b"/r/main/@ace/mod.cpp", "Range: bytes=500-\r\n").await;
    assert!(resp.starts_with("HTTP/1.1 416"), "{resp}");
    let resp = raw_get(srv.port, b"/r/main/.a3s/sync", "Range: bytes=0-1\r\n").await;
    assert!(resp.starts_with("HTTP/1.1 206"), "{resp}");
    let resp = raw_get(srv.port, b"/r/main/../../secret.key", "").await;
    assert!(
        resp.starts_with("HTTP/1.1 400") || resp.starts_with("HTTP/1.1 404"),
        "{resp}"
    );
    let resp = raw_get(srv.port, b"/r/main/.axs/build-cache.zst", "").await;
    assert!(resp.starts_with("HTTP/1.1 404"), "{resp}");

    // Delta update: modify the big file, delete one file.
    let mut big2 = big.clone();
    big2[1_000_000..1_000_200].fill(9);
    let j = srv
        .publish(
            "main",
            &[("@ace/addons/ace_main.pbo", big2.clone())],
            &["@ace/mod.cpp"],
        )
        .await;
    assert_eq!(j["summary"]["revision"], 3);
    let dest = tempfile::tempdir().unwrap();
    client_sync(&t, dest.path(), true).await;
    // (fresh copy of revision 3)
    std::fs::write(dest.path().join("@ace/addons/ace_main.pbo"), &big).unwrap();
    let (plan, rep, bytes) = client_sync(&t, dest.path(), true).await;
    assert!(rep.ok());
    assert_eq!(plan.fetch_count, 1);
    assert!(bytes < 100_000, "delta transferred {bytes}");
    assert!(!dest.path().join("@ace/mod.cpp").exists());

    // Events → rebuild → visible in the manifest.
    let r = srv
        .json(
            reqwest::Method::PUT,
            "/repos/main/events",
            json!([{"name":"Op","addons":[{"name":"@ace","optional":false}]}]),
        )
        .await;
    srv.wait_job(r["job"].as_i64().unwrap()).await;
    let snap = axs_sync::fetch_snapshot(&t, true).await.unwrap();
    assert_eq!(snap.manifest.events[0].name, "Op");
    let legacy = axs_sync::fetch_legacy(&t).await.unwrap();
    assert_eq!(legacy.manifest.events[0].addons[0].name, "@ace");
    assert!(
        !snap.manifest.content_updated,
        "metadata-only rebuild must not flag content changes"
    );

    // Private repository with Basic auth; embedded credential in autoconfig.
    let r = srv
        .json(
            reqwest::Method::POST,
            "/repos/main/credentials",
            json!({"login":"player","password":"s3cret-pass","embed":true}),
        )
        .await;
    srv.wait_job(r["job"].as_i64().unwrap()).await;
    let mut settings = srv
        .json(reqwest::Method::GET, "/repos/main", Value::Null)
        .await["settings"]
        .clone();
    settings["visibility"] = json!("private");
    srv.json(
        reqwest::Method::PATCH,
        "/repos/main",
        json!({"settings": settings, "rebuild": false}),
    )
    .await;
    let anon = srv.transport("main", None);
    assert!(matches!(
        anon.get_bytes(".a3s/sync").await,
        Err(axs_transport::TransportError::Unauthorized)
    ));
    let authed = srv.transport("main", Some(("player", "s3cret-pass")));
    let ac =
        axs_legacy::read_autoconfig(&authed.get_bytes(".a3s/autoconfig").await.unwrap()).unwrap();
    assert_eq!(ac.protocole.unwrap().login.as_deref(), Some("player"));
    settings["visibility"] = json!("public");
    srv.json(
        reqwest::Method::PATCH,
        "/repos/main",
        json!({"settings": settings, "rebuild": false}),
    )
    .await;

    // Mirror "main" into a second repository.
    let r = srv
        .json(
            reqwest::Method::POST,
            "/repos",
            json!({"slug":"copy","name":"Copy","settings": {"mirror": {"url": format!("{}/r/main", srv.base), "import_events": true}}}),
        )
        .await;
    srv.wait_job(r["job"].as_i64().unwrap()).await;
    let r = srv
        .json(reqwest::Method::POST, "/repos/copy/mirror", json!({}))
        .await;
    let j = srv.wait_job(r["job"].as_i64().unwrap()).await;
    assert_eq!(j["summary"]["files"], 4, "{j}");
    let copy = axs_sync::fetch_snapshot(&srv.transport("copy", None), true)
        .await
        .unwrap();
    assert_eq!(copy.manifest.files.len(), 4);
    assert_eq!(copy.manifest.events.len(), 1);
    // Mirroring again without upstream changes publishes nothing.
    let r = srv
        .json(reqwest::Method::POST, "/repos/copy/mirror", json!({}))
        .await;
    let j = srv.wait_job(r["job"].as_i64().unwrap()).await;
    assert_eq!(j["summary"]["changed"], false, "{j}");

    // Landing page and public API.
    let html = srv
        .http
        .get(format!("{}/repo/main", srv.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(html.contains("Main Repo") && html.contains("armaxsync://add?url="));
    let audit = srv.json(reqwest::Method::GET, "/audit", Value::Null).await;
    assert!(audit
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "changeset.commit"));
    let metrics = srv
        .http
        .get(format!("{}/metrics", srv.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(metrics.contains("armaxsync_bytes_served_total{repo=\"main\"}"));
}

/// Readers hammer the metadata while revisions are published back to back:
/// every response must parse and be internally consistent, and a final sync
/// must verify cleanly.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn publish_is_atomic_for_readers() {
    let srv = Arc::new(Srv::start().await);
    let r = srv
        .json(
            reqwest::Method::POST,
            "/repos",
            json!({"slug":"live","name":"Live"}),
        )
        .await;
    srv.wait_job(r["job"].as_i64().unwrap()).await;
    srv.publish(
        "live",
        &[
            ("@m/addons/a.pbo", pseudo(200_000, 1)),
            ("@m/addons/b.pbo", pseudo(100_000, 2)),
        ],
        &[],
    )
    .await;

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut readers = vec![];
    for i in 0..12 {
        let (srv, stop) = (srv.clone(), stop.clone());
        readers.push(tokio::spawn(async move {
            let t = srv.transport("live", None);
            let mut n = 0;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                if i % 2 == 0 {
                    // Modern: index → manifest hash must always match.
                    let snap = axs_sync::fetch_snapshot(&t, true)
                        .await
                        .expect("modern snapshot");
                    assert!(!snap.manifest.files.is_empty());
                } else {
                    // Legacy: every .a3s file parses.
                    let sync = axs_legacy::read_sync(&t.get_bytes(".a3s/sync").await.unwrap())
                        .expect("sync parses");
                    let si = axs_legacy::read_server_info(
                        &t.get_bytes(".a3s/serverinfo").await.unwrap(),
                    )
                    .expect("serverinfo parses");
                    assert!(si.revision > 0 && sync.count_leaves() >= 2);
                }
                n += 1;
            }
            n
        }));
    }
    for k in 0..12u32 {
        srv.publish(
            "live",
            &[
                ("@m/addons/a.pbo", pseudo(200_000, 100 + k)),
                (&format!("@m/addons/v{k}.pbo"), pseudo(5000, k)),
            ],
            &[],
        )
        .await;
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut total = 0;
    for r in readers {
        total += r.await.expect("reader panicked");
    }
    assert!(total > 50, "readers made only {total} requests");
    let dest = tempfile::tempdir().unwrap();
    let t = srv.transport("live", None);
    let (_, rep, _) = client_sync(&t, dest.path(), true).await;
    assert!(rep.ok());
    let (plan, _, _) = client_sync(&t, dest.path(), true).await;
    assert_eq!(
        plan.fetch_count, 0,
        "destination verifies against the final revision"
    );
}

/// Desktop delta push: initial upload, then a small change sent as a delta.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delta_push() {
    use axs_client::push::{push, PushOptions, PushTarget};
    let srv = Srv::start().await;
    let r = srv
        .json(
            reqwest::Method::POST,
            "/repos",
            json!({"slug":"pushed","name":"Pushed"}),
        )
        .await;
    srv.wait_job(r["job"].as_i64().unwrap()).await;
    let src = tempfile::tempdir().unwrap();
    let big = pseudo(4_000_000, 7);
    std::fs::create_dir_all(src.path().join("@mod/addons")).unwrap();
    std::fs::write(src.path().join("@mod/addons/big.pbo"), &big).unwrap();
    std::fs::write(src.path().join("@mod/addons/copy.pbo"), &big).unwrap();
    std::fs::write(src.path().join("@mod/mod.cpp"), b"name=x;").unwrap();
    let target = PushTarget {
        server: srv.base.clone(),
        repo: "pushed".into(),
        token: srv.token.clone(),
    };
    let opts = PushOptions {
        commit: true,
        delete_missing: true,
        ..Default::default()
    };
    let ev = Arc::new(|_| {});
    let r1 = push(&target, src.path(), &opts, None, ev.clone())
        .await
        .unwrap();
    assert_eq!(r1.full_files, 3, "{r1:?}");
    assert!(r1.revision.is_some());

    // Small edit + a deletion: the big file goes as a delta.
    let mut big2 = big.clone();
    big2[2_000_000..2_000_064].fill(1);
    big2.extend_from_slice(b"appended tail");
    std::fs::write(src.path().join("@mod/addons/big.pbo"), &big2).unwrap();
    std::fs::remove_file(src.path().join("@mod/addons/copy.pbo")).unwrap();
    let r2 = push(&target, src.path(), &opts, None, ev).await.unwrap();
    assert_eq!(r2.delta_files, 1, "{r2:?}");
    assert_eq!(r2.deleted, 1);
    assert!(r2.bytes_sent < 100_000, "sent {} bytes", r2.bytes_sent);

    let dest = tempfile::tempdir().unwrap();
    let (_, rep, _) = client_sync(&srv.transport("pushed", None), dest.path(), true).await;
    assert!(rep.ok());
    assert_eq!(
        std::fs::read(dest.path().join("@mod/addons/big.pbo")).unwrap(),
        big2
    );
    assert!(!dest.path().join("@mod/addons/copy.pbo").exists());
}
