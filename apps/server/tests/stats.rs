//! Download statistics end to end: requests through a trusted proxy with
//! different clients, metadata checks, full / partial / aborted downloads,
//! errors; then every /stats endpoint, access scoping, and that no client
//! address reaches the database.

use std::net::SocketAddr;
use std::time::Duration;

use armaxsync_server::auth::{create_user, Role};
use armaxsync_server::{api, build_state, config::Config};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct S {
    base: String,
    port: u16,
    http: reqwest::Client,
    cookie: String,
    csrf: String,
}

impl S {
    async fn login(base: &str, port: u16, user: &str) -> S {
        let http = reqwest::Client::new();
        let r = http
            .post(format!("{base}/api/v1/auth/login"))
            .json(&json!({"username": user, "password": "correct horse"}))
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
        S {
            base: base.into(),
            port,
            http,
            cookie,
            csrf: me["csrf"].as_str().unwrap().into(),
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
        let st = r.status().as_u16();
        (st, r.json().await.unwrap_or(Value::Null))
    }

    async fn get(&self, path: &str) -> Value {
        let (s, v) = self.call(reqwest::Method::GET, path, None).await;
        assert_eq!(s, 200, "{path}: {v}");
        v
    }

    async fn wait_job(&self, id: i64) {
        for _ in 0..400 {
            let j = self.get(&format!("/jobs/{id}")).await;
            if j["status"] == "succeeded" {
                return;
            }
            assert!(
                matches!(j["status"].as_str(), Some("queued" | "running")),
                "{j}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("job {id} timed out");
    }

    /// GET a repository file as a player behind the proxy.
    async fn fetch(&self, path: &str, ua: &str, ip: &str, range: Option<&str>) -> u16 {
        let mut rb = self
            .http
            .get(format!("{}/r/main/{path}", self.base))
            .header("user-agent", ua)
            .header("x-forwarded-for", ip);
        if let Some(r) = range {
            rb = rb.header("range", r);
        }
        let r = rb.send().await.unwrap();
        let s = r.status().as_u16();
        let _ = r.bytes().await;
        s
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn statistics_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let cfg = Config {
        data_dir: dir.path().to_path_buf(),
        public_url: format!("http://127.0.0.1:{port}"),
        ..Default::default()
    };
    let st = build_state(cfg).await.unwrap();
    create_user(&st, "owner", "correct horse", Role::Owner)
        .await
        .unwrap();
    create_user(&st, "mike", "correct horse", Role::User)
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
    let o = S::login(&base, port, "owner").await;
    use reqwest::Method as M;

    // A repository with two addons; slow it down so a download can be aborted.
    let (s, _) = o
        .call(
            M::POST,
            "/repos",
            Some(json!({"slug": "main", "name": "Main", "game": "arma3", "mode": "managed"})),
        )
        .await;
    assert_eq!(s, 201);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let cs = o
        .call(M::POST, "/repos/main/changesets", Some(json!({})))
        .await
        .1["id"]
        .as_i64()
        .unwrap();
    for (p, n) in [
        ("@ace/addons/ace_main.pbo", 2000usize),
        ("@cba/addons/cba.pbo", 500),
        ("@big/addons/big.pbo", 2 << 20),
    ] {
        let r = o
            .http
            .put(format!("{base}/api/v1/changesets/{cs}/files/{p}"))
            .header("cookie", &o.cookie)
            .header("x-csrf-token", &o.csrf)
            .body(vec![7u8; n])
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success());
    }
    let job = o
        .call(
            M::POST,
            &format!("/changesets/{cs}/commit"),
            Some(json!({})),
        )
        .await
        .1["job"]
        .as_i64()
        .unwrap();
    o.wait_job(job).await;
    let (s, _) = o
        .call(
            M::PATCH,
            "/repos/main",
            Some(json!({"settings": {"bandwidth_limit": 200_000}, "rebuild": false})),
        )
        .await;
    assert_eq!(s, 200);

    // Players: 203.0.113.1 (legacy client), 203.0.113.2 (ArmaXSync). A browser
    // and a direct (non-proxied) request also arrive.
    assert_eq!(
        o.fetch(".a3s/serverinfo", "ArmA3Sync", "203.0.113.1", None)
            .await,
        200
    );
    assert_eq!(
        o.fetch(".axs/index.json", "ArmaXSync/0.2.0", "203.0.113.2", None)
            .await,
        200
    );
    assert_eq!(
        o.fetch(
            "@ace/addons/ace_main.pbo",
            "ArmaXSync/0.2.0",
            "203.0.113.2",
            None
        )
        .await,
        200
    );
    assert_eq!(
        o.fetch(
            "@cba/addons/cba.pbo",
            "ArmA3Sync",
            "203.0.113.1",
            Some("bytes=0-9")
        )
        .await,
        206
    );
    assert_eq!(
        o.fetch(
            "@cba/addons/missing.pbo",
            "ArmaXSync/0.2.0",
            "203.0.113.2",
            None
        )
        .await,
        404
    );
    assert_eq!(
        o.fetch(
            "@ace/addons/ace_main.pbo",
            "Mozilla/5.0 (X11)",
            "198.51.100.7",
            None
        )
        .await,
        200
    );

    // Aborted: read a little of the slow 2 MB file, then hang up.
    {
        let mut sock = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        sock.write_all(b"GET /r/main/@big/addons/big.pbo HTTP/1.1\r\nHost: x\r\nUser-Agent: ArmaXSync/0.1.0\r\nX-Forwarded-For: 203.0.113.2\r\n\r\n")
            .await
            .unwrap();
        let mut buf = vec![0u8; 32 * 1024];
        let _ = sock.read(&mut buf).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let day = 86_400_000i64;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let range = format!("from={}&to={}", now - 7 * day, now + 60_000);

    let sum = o.get(&format!("/stats/summary?{range}")).await;
    let c = &sum["current"];
    assert_eq!(c["checks"], 2, "{sum}");
    assert_eq!(c["requests"], 7, "{sum}");
    assert_eq!(
        c["files"], 2,
        "full downloads: ace twice (player + browser): {sum}"
    );
    assert_eq!(c["ranges"], 1);
    assert_eq!(c["s404"], 1);
    assert_eq!(c["s206"], 1);
    assert_eq!(c["aborted"], 1, "{sum}");
    assert_eq!(c["completed"], 3, "{sum}");
    assert!(c["bytes_meta"].as_u64().unwrap() > 0);
    assert_eq!(c["c_axs"], 4);
    assert_eq!(c["c_a3s"], 2);
    assert_eq!(c["c_browser"], 1);
    assert_eq!(c["players"], 2, "browser not counted: {sum}");
    assert!(sum["daily"].as_array().unwrap().len() >= 7);

    let tr = o.get(&format!("/stats/traffic?{range}&bucket=hour")).await;
    assert_eq!(tr["bucket"], "hour");
    assert!(tr["total"]["served"].as_u64().unwrap() > 4000);
    assert!(tr["by_repo"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["repo"] == "main"));

    let pl = o.get(&format!("/stats/players?{range}")).await;
    assert_eq!(pl["total"], 2, "{pl}");
    assert_eq!(pl["per_repo"][0]["players"], 2);
    let last = pl["daily"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (last["players"].as_u64(), last["new"].as_u64()),
        (Some(2), Some(2)),
        "{pl}"
    );

    let cl = o.get(&format!("/stats/clients?{range}")).await;
    assert!(
        cl["versions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["family"] == "armaxsync" && v["version"] == "0.2.0"),
        "{cl}"
    );

    let ad = o.get(&format!("/stats/addons?{range}")).await;
    let ace = ad["addons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["addon"] == "@ace")
        .unwrap()
        .clone();
    assert_eq!(ace["downloads"], 2, "{ad}");
    assert_eq!(ace["bytes"], 4000);
    assert!(ad["addons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["addon"] == "@big"));

    let hm = o.get(&format!("/stats/heatmap?{range}&tz=120")).await;
    assert_eq!(hm["cells"].as_array().unwrap().len(), 168);
    assert!(
        hm["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["requests"].as_u64().unwrap())
            .sum::<u64>()
            >= 7
    );

    let rp = o.get(&format!("/stats/repos?{range}")).await;
    assert!(
        !rp["revisions"]["main"].as_array().unwrap().is_empty(),
        "{rp}"
    );
    assert!(
        rp["jobs"]["publish"]["succeeded"].as_u64().unwrap() >= 1,
        "{rp}"
    );

    let lv = o.get("/stats/live").await;
    assert_eq!(lv["downloads"], 0);
    assert!(lv["players"].as_u64().unwrap() >= 2);

    // Legacy endpoint keeps working.
    let legacy = o.get("/stats").await;
    assert!(legacy["daily"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["repo"] == "main"));

    // A user who doesn't maintain the repository sees nothing of it.
    let mike = S::login(&base, port, "mike").await;
    let (s, _) = mike
        .call(M::GET, &format!("/stats/summary?{range}&repo=main"), None)
        .await;
    assert_eq!(s, 404);
    let ms = mike.get(&format!("/stats/summary?{range}")).await;
    assert_eq!(ms["current"]["requests"], 0);
    assert_eq!(ms["current"]["players"], 0);
    let _ = mike.port;

    // No client address is stored anywhere.
    drop(st);
    let db = std::fs::read(dir.path().join("armaxsync.db")).unwrap();
    let wal = std::fs::read(dir.path().join("armaxsync.db-wal")).unwrap_or_default();
    for ip in ["203.0.113.1", "203.0.113.2", "198.51.100.7"] {
        let needle = ip.as_bytes();
        for blob in [&db, &wal] {
            assert!(
                !blob.windows(needle.len()).any(|w| w == needle),
                "{ip} found in the database"
            );
        }
    }
}
