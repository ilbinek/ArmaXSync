//! Output helpers: JSON lines or human-readable text, and progress bars.

use indicatif::{ProgressBar, ProgressStyle};

pub fn json_line(v: &serde_json::Value) {
    println!("{}", serde_json::to_string(v).unwrap_or_default());
}

pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.2} {}", UNITS[u])
    }
}

pub fn bar(quiet: bool, len: u64, msg: &str) -> ProgressBar {
    if quiet {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::with_template(
            "{msg:>12} [{bar:30}] {bytes}/{total_bytes} {bytes_per_sec} eta {eta}",
        )
        .unwrap()
        .progress_chars("=> "),
    );
    pb.set_message(msg.to_owned());
    pb
}
