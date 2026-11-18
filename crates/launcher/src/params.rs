//! Run parameters, built exactly like ArmA3Sync's `LaunchService`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::addons::Addon;
use crate::profile::{FavoriteServer, Profile};

/// Problems that prevent launching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchCheck {
    pub missing_addons: Vec<String>,
    pub duplicate_selection: Vec<String>,
}

impl LaunchCheck {
    pub fn ok(&self) -> bool {
        self.missing_addons.is_empty() && self.duplicate_selection.is_empty()
    }
}

/// Options part of the command line (legacy `getRunParameters`).
pub fn option_parameters(p: &Profile) -> Vec<String> {
    let o = &p.options;
    let mut v = vec![];
    if let Some(m) = o.malloc.as_ref().filter(|m| !m.is_empty()) {
        v.push(format!("-malloc={m}"));
    }
    if let Some(n) = o
        .game_profile
        .as_ref()
        .filter(|n| !n.is_empty() && *n != "Default")
    {
        v.push(format!("-name={n}"));
    }
    let flags = [
        (o.show_script_errors, "-showScriptErrors"),
        (o.no_pause, "-noPause"),
        (o.file_patching, "-filePatching"),
        (o.window_mode, "-window"),
        (o.check_signatures, "-checkSignatures"),
    ];
    v.extend(flags.iter().filter(|f| f.0).map(|f| f.1.to_owned()));
    if let Some(m) = o.max_mem.filter(|&m| m > 0) {
        v.push(format!("-maxMem={m}"));
    }
    if let Some(c) = o.cpu_count.filter(|&c| c > 0) {
        v.push(format!("-cpuCount={c}"));
    }
    if let Some(t) = o.ex_threads {
        v.push(format!("-exThreads={t}"));
    }
    let flags2 = [
        (o.enable_ht, "-enableHT"),
        (o.huge_pages, "-hugepages"),
        (o.no_splash, "-nosplash"),
        (o.default_world_empty, "-world=empty"),
        (o.no_logs, "-nologs"),
    ];
    v.extend(flags2.iter().filter(|f| f.0).map(|f| f.1.to_owned()));
    v
}

/// Selected addon names in load order: priority list first, then the rest
/// alphabetically (all lowercase, de-duplicated).
pub fn load_order(p: &Profile) -> Vec<String> {
    let mut selected: Vec<String> = p
        .selected_addons()
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    selected.dedup();
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for pr in &p.addon_priority {
        let k = pr.to_lowercase();
        if selected.contains(&k) && seen.insert(k.clone()) {
            out.push(k);
        }
    }
    let mut rest: Vec<String> = selected.into_iter().filter(|s| !seen.contains(s)).collect();
    rest.sort();
    rest.dedup();
    out.extend(rest);
    out
}

/// Check the profile's selection against the available addons.
pub fn check(p: &Profile, addons: &BTreeMap<String, Addon>) -> LaunchCheck {
    let mut missing = vec![];
    let mut seen = std::collections::HashMap::new();
    for n in p.selected_addons() {
        let k = n.to_lowercase();
        *seen.entry(k.clone()).or_insert(0) += 1;
        if !addons.contains_key(&k) && !missing.contains(&n) {
            missing.push(n);
        }
    }
    let mut dup: Vec<String> = seen
        .into_iter()
        .filter(|(_, c)| *c > 1)
        .map(|(k, _)| k)
        .collect();
    dup.sort();
    LaunchCheck {
        missing_addons: missing,
        duplicate_selection: dup,
    }
}

/// Converts a host path for the game (identity, or `Z:\…` under Proton).
pub type PathMapper = dyn Fn(&Path) -> String;

/// Linux path → Wine/Proton `Z:` drive path.
pub fn proton_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if s.starts_with('/') {
        format!("Z:{}", s.replace('/', "\\"))
    } else {
        s.into_owned()
    }
}

pub fn native_path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let n = |p: &Path| {
        p.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    n(a) == n(b)
}

/// `-mod` arguments: consecutive addons located in the game folder are
/// grouped as relative names, others get their own absolute `-mod=` entry.
pub fn mod_parameters(
    p: &Profile,
    addons: &BTreeMap<String, Addon>,
    map: &PathMapper,
) -> Vec<String> {
    let game_dir = p.game_dir().map(Path::to_path_buf);
    let mut out = vec![];
    let mut group: Vec<String> = vec![];
    let flush = |group: &mut Vec<String>, out: &mut Vec<String>| {
        if !group.is_empty() {
            out.push(format!("-mod={};", group.join(";")));
            group.clear();
        }
    };
    for k in load_order(p) {
        let Some(a) = addons.get(&k) else { continue };
        let at_root = game_dir
            .as_deref()
            .map(|g| same_dir(a.parent(), g))
            .unwrap_or(false);
        if at_root {
            group.push(a.name.clone());
        } else {
            flush(&mut group, &mut out);
            out.push(format!("-mod={};", map(&a.path)));
        }
    }
    flush(&mut group, &mut out);
    out
}

/// Full argument list (without the executable).
pub fn run_parameters(
    p: &Profile,
    addons: &BTreeMap<String, Addon>,
    server: Option<&FavoriteServer>,
    map: &PathMapper,
) -> Vec<String> {
    let mut extra: Vec<String> = p
        .additional_parameters
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    // Legacy: malloc parameters always come first.
    if let Some(i) = extra
        .iter()
        .position(|s| s.to_lowercase().contains("malloc"))
    {
        let m = extra.remove(i);
        extra.insert(0, m);
    }
    let mut v = extra;
    v.extend(option_parameters(p));
    if let Some(s) = server {
        v.push(format!("-connect={}", s.host));
        v.push(format!("-port={}", s.port));
        if let Some(pw) = s.password.as_ref().filter(|p| !p.is_empty()) {
            v.push(format!("-password={pw}"));
        }
    }
    v.extend(mod_parameters(p, addons, map));
    v
}

/// Arguments for `arma3server`: `-mod=` and `-serverMod=` lines (semicolon lists).
pub fn server_mod_line(mods: &[PathBuf], server_mods: &[PathBuf], map: &PathMapper) -> Vec<String> {
    let mut v = vec![];
    if !mods.is_empty() {
        v.push(format!(
            "-mod={}",
            mods.iter().map(|m| map(m)).collect::<Vec<_>>().join(";")
        ));
    }
    if !server_mods.is_empty() {
        v.push(format!(
            "-serverMod={}",
            server_mods
                .iter()
                .map(|m| map(m))
                .collect::<Vec<_>>()
                .join(";")
        ));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::AddonSource;
    use crate::profile::{GroupNode, LauncherOptions};

    fn addon(name: &str, parent: &str) -> (String, Addon) {
        let p = PathBuf::from(parent).join(name);
        (
            name.to_lowercase(),
            Addon {
                key: name.to_lowercase(),
                name: name.into(),
                display_name: name.into(),
                path: p,
                source: AddonSource::Local,
            },
        )
    }

    #[test]
    fn legacy_parameter_order() {
        let game = "C:/Games/Arma 3";
        let addons: BTreeMap<String, Addon> = [
            addon("@CBA_A3", game),
            addon("@ace", game),
            addon("@task_force_radio", "D:/mods"),
            addon("@rhs", game),
        ]
        .into_iter()
        .collect();
        let p = Profile {
            executable: Some(PathBuf::from(format!("{game}/arma3_x64.exe"))),
            options: LauncherOptions {
                game_profile: Some("Shepard".into()),
                no_splash: true,
                default_world_empty: true,
                show_script_errors: true,
                max_mem: Some(2047),
                ex_threads: Some(7),
                malloc: Some("tbb4malloc_bi_x64".into()),
                ..Default::default()
            },
            additional_parameters: "-skipIntro -malloc=custom".into(),
            addon_priority: vec!["@CBA_A3".into(), "@ace".into()],
            groups: vec![GroupNode::Group {
                name: "G".into(),
                modset: None,
                children: ["@ace", "@CBA_A3", "@rhs", "@task_force_radio", "@missing"]
                    .iter()
                    .map(|n| GroupNode::Addon {
                        name: (*n).into(),
                        selected: true,
                        optional: false,
                    })
                    .collect(),
            }],
            ..Default::default()
        };
        let server = FavoriteServer {
            name: "S".into(),
            host: "203.0.113.5".into(),
            port: 2302,
            password: Some("pw".into()),
            ..Default::default()
        };
        let v = run_parameters(&p, &addons, Some(&server), &native_path);
        let tfar = format!(
            "-mod={};",
            PathBuf::from("D:/mods").join("@task_force_radio").display()
        );
        assert_eq!(
            v,
            [
                "-malloc=custom",
                "-skipIntro",
                "-malloc=tbb4malloc_bi_x64",
                "-name=Shepard",
                "-showScriptErrors",
                "-maxMem=2047",
                "-exThreads=7",
                "-nosplash",
                "-world=empty",
                "-connect=203.0.113.5",
                "-port=2302",
                "-password=pw",
                "-mod=@CBA_A3;@ace;@rhs;",
                tfar.as_str(),
            ]
        );
        let c = check(&p, &addons);
        assert_eq!(c.missing_addons, ["@missing"]);
    }

    #[test]
    fn proton_paths() {
        assert_eq!(
            proton_path(Path::new("/home/u/mods/@ace")),
            "Z:\\home\\u\\mods\\@ace"
        );
    }
}
