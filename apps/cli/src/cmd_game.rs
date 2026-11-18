//! Launcher commands: launch, profile, addons, modset, servers, server-helper, rpt.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use axs_launcher::{FavoriteServer, GroupNode, LaunchMode, Profile};

use crate::{exit, output, Ctx};

#[derive(Args)]
pub struct LaunchArgs {
    /// Profile (default: the active profile).
    #[arg(long)]
    profile: Option<String>,
    /// Join a favorite server.
    #[arg(long)]
    server: Option<String>,
    /// Print the command instead of starting the game.
    #[arg(long)]
    dry_run: bool,
    /// Start even if addons are missing or Arma is already running.
    #[arg(long)]
    force: bool,
}

#[derive(Subcommand)]
pub enum ProfileCmd {
    List,
    Show {
        name: Option<String>,
    },
    /// Create a profile (optionally copying another).
    New {
        name: String,
        #[arg(long)]
        copy: Option<String>,
    },
    Remove {
        name: String,
    },
    /// Make a profile the active one.
    Use {
        name: String,
    },
    /// Change profile settings.
    Set(ProfileSet),
}

#[derive(Args)]
pub struct ProfileSet {
    name: String,
    #[arg(long)]
    exe: Option<PathBuf>,
    #[arg(long, value_parser = ["direct", "steam"])]
    mode: Option<String>,
    #[arg(long)]
    proton: Option<bool>,
    /// Add an addon search folder (repeatable).
    #[arg(long = "add-dir")]
    add_dir: Vec<PathBuf>,
    #[arg(long = "remove-dir")]
    remove_dir: Vec<PathBuf>,
    #[arg(long)]
    workshop: Option<bool>,
    /// Additional parameters (replaces).
    #[arg(long, allow_hyphen_values = true)]
    params: Option<String>,
    /// Load order (comma separated addon names).
    #[arg(long)]
    priority: Option<String>,
    /// Select addons (comma separated; creates/updates group "Selected").
    #[arg(long)]
    select: Option<String>,
    /// Deselect everything first.
    #[arg(long)]
    clear: bool,
    #[arg(long = "game-profile")]
    game_profile: Option<String>,
    /// Launcher options: name=value (show-script-errors, no-pause, window, file-patching,
    /// check-signatures, max-mem, cpu-count, ex-threads, malloc, enable-ht, huge-pages,
    /// no-splash, world-empty, no-logs, auto-restart).
    #[arg(long = "opt")]
    opts: Vec<String>,
}

#[derive(Subcommand)]
pub enum ServerCmd {
    List,
    Add {
        name: String,
        /// host:port
        address: String,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        modset: Option<String>,
    },
    Remove {
        name: String,
    },
}

#[derive(Args)]
pub struct ModsetArgs {
    /// Repository name.
    repo: String,
    /// Event of the repository (default: whole repository).
    #[arg(long)]
    event: Option<String>,
    #[arg(long)]
    profile: Option<String>,
}

#[derive(Args)]
pub struct ServerHelperArgs {
    /// Dedicated server folder (arma3server_x64 location).
    server_dir: PathBuf,
    /// Addons loaded with -mod (comma separated).
    #[arg(long)]
    mods: String,
    /// Addons loaded with -serverMod (comma separated).
    #[arg(long, default_value = "")]
    server_mods: String,
    /// Folder for the mods (default: <server>/mods).
    #[arg(long)]
    target: Option<PathBuf>,
    /// Copy instead of linking.
    #[arg(long)]
    copy: bool,
    /// Lowercase file names (Linux servers); implies copy.
    #[arg(long)]
    lowercase: bool,
    #[arg(long)]
    profile: Option<String>,
}

#[derive(Args)]
pub struct RptArgs {
    /// RPT file (default: newest).
    file: Option<PathBuf>,
    /// Keep following the file.
    #[arg(short, long)]
    follow: bool,
    #[arg(long)]
    proton: bool,
}

fn names(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_owned)
        .collect()
}

pub async fn launch(a: LaunchArgs, ctx: &Ctx) -> anyhow::Result<i32> {
    let client = ctx.client()?;
    let p = match &a.profile {
        Some(n) => client.profile(n),
        None => client.active_profile(),
    };
    let addons = client.addons(&p);
    let chk = axs_launcher::check(&p, &addons);
    let cmd = match client.launch_command(&p, a.server.as_deref()) {
        Ok(c) => c,
        Err(e) if a.dry_run => {
            // Still show what would be passed to the game.
            let srv = a
                .server
                .as_deref()
                .or(p.server.as_deref())
                .and_then(|n| client.favorite_server(n));
            let args =
                axs_launcher::run_parameters(&p, &addons, srv, &axs_launcher::params::native_path);
            if ctx.json {
                output::json_line(
                    &serde_json::json!({"profile": p.name, "error": e.to_string(), "args": args, "check": chk}),
                );
            } else {
                eprintln!("warning: {e}");
                println!("<executable> {}", args.join(" "));
            }
            return Ok(exit::VERIFY_FAILED);
        }
        Err(e) => return Err(e.into()),
    };
    if ctx.json {
        output::json_line(&serde_json::json!({"profile": p.name, "command": cmd, "check": chk}));
    } else {
        if !chk.missing_addons.is_empty() {
            eprintln!("missing addons: {}", chk.missing_addons.join(", "));
        }
        if !chk.duplicate_selection.is_empty() {
            eprintln!(
                "duplicated selection: {}",
                chk.duplicate_selection.join(", ")
            );
        }
        println!("{} {}", cmd.program.display(), cmd.args.join(" "));
    }
    if a.dry_run {
        return Ok(if chk.ok() {
            exit::OK
        } else {
            exit::VERIFY_FAILED
        });
    }
    if !a.force {
        anyhow::ensure!(chk.ok(), "fix missing/duplicated addons or use --force");
        anyhow::ensure!(
            !axs_launcher::detect::arma_running(),
            "Arma 3 is already running (use --force)"
        );
    }
    for (n, e) in axs_launcher::start_external_apps(&client.config.external_apps) {
        eprintln!("external application {n}: {e}");
    }
    let pid = axs_launcher::launch(&cmd, p.options.auto_restart, p.proton)?;
    if !ctx.json {
        println!("started (pid {pid})");
    }
    if p.options.auto_restart && p.launch_mode == LaunchMode::Direct {
        // Keep the process alive for the restart watcher.
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
    Ok(exit::OK)
}

fn print_profile(p: &Profile, client: &axs_client::Client, ctx: &Ctx) {
    if ctx.json {
        output::json_line(&serde_json::to_value(p).unwrap_or_default());
        return;
    }
    println!("Profile {}", p.name);
    println!(
        "  executable: {}",
        p.executable
            .as_ref()
            .map(|e| e.display().to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  launch:     {:?}{}",
        p.launch_mode,
        if p.proton { " (Proton)" } else { "" }
    );
    for d in &p.addon_search_dirs {
        println!("  search dir: {}", d.display());
    }
    if !p.addon_priority.is_empty() {
        println!("  priority:   {}", p.addon_priority.join(", "));
    }
    let addons = client.addons(p);
    fn tree(
        n: &GroupNode,
        depth: usize,
        addons: &std::collections::BTreeMap<String, axs_launcher::Addon>,
    ) {
        match n {
            GroupNode::Group {
                name,
                children,
                modset,
            } => {
                println!(
                    "  {}{name}/{}",
                    "  ".repeat(depth),
                    if modset.is_some() { " (modset)" } else { "" }
                );
                children.iter().for_each(|c| tree(c, depth + 1, addons));
            }
            GroupNode::Addon {
                name,
                selected,
                optional,
            } => println!(
                "  {}[{}] {name}{}{}",
                "  ".repeat(depth),
                if *selected { "x" } else { " " },
                if *optional { " (optional)" } else { "" },
                if addons.contains_key(&name.to_lowercase()) {
                    ""
                } else {
                    "  <missing>"
                }
            ),
        }
    }
    p.groups.iter().for_each(|g| tree(g, 0, &addons));
    let params = axs_launcher::params::option_parameters(p);
    println!("  options:    {}", params.join(" "));
    if !p.additional_parameters.is_empty() {
        println!("  extra:      {}", p.additional_parameters);
    }
}

fn set_opt(p: &mut Profile, kv: &str) -> anyhow::Result<()> {
    let (k, v) = kv.split_once('=').unwrap_or((kv, "true"));
    let b = || -> anyhow::Result<bool> { Ok(v.parse()?) };
    let n = || -> anyhow::Result<Option<u32>> {
        Ok(if v.is_empty() || v == "0" || v == "none" {
            None
        } else {
            Some(v.parse()?)
        })
    };
    let o = &mut p.options;
    match k {
        "show-script-errors" => o.show_script_errors = b()?,
        "no-pause" => o.no_pause = b()?,
        "window" => o.window_mode = b()?,
        "file-patching" => o.file_patching = b()?,
        "check-signatures" => o.check_signatures = b()?,
        "max-mem" => o.max_mem = n()?,
        "cpu-count" => o.cpu_count = n()?,
        "ex-threads" => {
            o.ex_threads = if v.is_empty() || v == "none" {
                None
            } else {
                Some(v.parse()?)
            }
        }
        "malloc" => o.malloc = Some(v.to_owned()).filter(|v| !v.is_empty() && v != "none"),
        "enable-ht" => o.enable_ht = b()?,
        "huge-pages" => o.huge_pages = b()?,
        "no-splash" => o.no_splash = b()?,
        "world-empty" => o.default_world_empty = b()?,
        "no-logs" => o.no_logs = b()?,
        "auto-restart" => o.auto_restart = b()?,
        other => anyhow::bail!("unknown option {other:?}"),
    }
    Ok(())
}

pub fn profile(c: ProfileCmd, ctx: &Ctx) -> anyhow::Result<i32> {
    let mut client = ctx.client()?;
    let store = client.profiles();
    match c {
        ProfileCmd::List => {
            let active = client
                .config
                .profile
                .clone()
                .unwrap_or_else(|| "Default".into());
            for p in store.list() {
                if ctx.json {
                    output::json_line(
                        &serde_json::json!({"name": p.name, "active": p.name == active}),
                    );
                } else {
                    println!("{} {}", if p.name == active { "*" } else { " " }, p.name);
                }
            }
        }
        ProfileCmd::Show { name } => {
            let p = match name {
                Some(n) => client.profile(&n),
                None => client.active_profile(),
            };
            print_profile(&p, &client, ctx);
        }
        ProfileCmd::New { name, copy } => {
            anyhow::ensure!(store.get(&name).is_none(), "profile {name} exists");
            let mut p = match copy {
                Some(c) => client.profile(&c),
                None => client.profile("Default"),
            };
            if store.get("Default").is_none() {
                store.save(&p)?;
            }
            p.name = name;
            store.save(&p)?;
        }
        ProfileCmd::Remove { name } => store.remove(&name)?,
        ProfileCmd::Use { name } => {
            anyhow::ensure!(store.get(&name).is_some(), "no profile {name}");
            client.config.profile = Some(name);
            client.save_config()?;
        }
        ProfileCmd::Set(s) => {
            let mut p = client.profile(&s.name);
            if let Some(e) = s.exe {
                p.executable = Some(e);
            }
            if let Some(m) = s.mode {
                p.launch_mode = if m == "steam" {
                    LaunchMode::Steam
                } else {
                    LaunchMode::Direct
                };
            }
            if let Some(v) = s.proton {
                p.proton = v;
            }
            if let Some(v) = s.workshop {
                p.use_workshop = v;
            }
            for d in s.add_dir {
                if !p.addon_search_dirs.contains(&d) {
                    p.addon_search_dirs.push(d);
                }
            }
            p.addon_search_dirs.retain(|d| !s.remove_dir.contains(d));
            if let Some(v) = s.params {
                p.additional_parameters = v;
            }
            if let Some(v) = s.priority {
                p.addon_priority = names(&v);
            }
            if let Some(v) = s.game_profile {
                p.options.game_profile = Some(v).filter(|x| !x.is_empty() && x != "Default");
            }
            for kv in &s.opts {
                set_opt(&mut p, kv)?;
            }
            if s.clear {
                p.groups.iter_mut().for_each(|g| g.set_selected(false));
            }
            if let Some(sel) = s.select {
                let list = names(&sel);
                let pos = p.groups.iter().position(|g| matches!(g, GroupNode::Group { name, modset: None, .. } if name == "Selected"));
                let mut g = pos.map(|i| p.groups.remove(i)).unwrap_or(GroupNode::Group {
                    name: "Selected".into(),
                    children: vec![],
                    modset: None,
                });
                if let GroupNode::Group { children, .. } = &mut g {
                    for n in list {
                        match children
                            .iter_mut()
                            .find(|c| c.name().eq_ignore_ascii_case(&n))
                        {
                            Some(GroupNode::Addon { selected, .. }) => *selected = true,
                            _ => children.push(GroupNode::Addon {
                                name: n,
                                selected: true,
                                optional: false,
                            }),
                        }
                    }
                }
                p.groups.push(g);
            }
            store.save(&p)?;
            print_profile(&p, &client, ctx);
        }
    }
    Ok(exit::OK)
}

pub fn addons(profile: Option<String>, ctx: &Ctx) -> anyhow::Result<i32> {
    let client = ctx.client()?;
    let p = match profile {
        Some(n) => client.profile(&n),
        None => client.active_profile(),
    };
    for a in client.addons(&p).values() {
        if ctx.json {
            output::json_line(&serde_json::to_value(a)?);
        } else {
            println!(
                "{:<32} {}{}",
                a.key,
                a.path.display(),
                if a.display_name != a.name {
                    format!("  ({})", a.display_name)
                } else {
                    String::new()
                }
            );
        }
    }
    Ok(exit::OK)
}

pub async fn modset(a: ModsetArgs, ctx: &Ctx) -> anyhow::Result<i32> {
    let mut client = ctx.client()?;
    let prof = a
        .profile
        .clone()
        .or(client.config.profile.clone())
        .unwrap_or_else(|| "Default".into());
    let p = client
        .apply_modset(&prof, &a.repo, a.event.as_deref())
        .await?;
    if !ctx.json {
        println!(
            "Modset {} added to profile {}.",
            a.event.as_deref().unwrap_or(&a.repo),
            p.name
        );
    }
    Ok(exit::OK)
}

pub fn servers(c: ServerCmd, ctx: &Ctx) -> anyhow::Result<i32> {
    let mut client = ctx.client()?;
    match c {
        ServerCmd::List => {
            for s in &client.config.favorite_servers {
                if ctx.json {
                    output::json_line(&serde_json::to_value(s)?);
                } else {
                    println!(
                        "{:<28} {}:{}{}",
                        s.name,
                        s.host,
                        s.port,
                        s.modset
                            .as_ref()
                            .map(|m| format!("  modset {m}"))
                            .unwrap_or_default()
                    );
                }
            }
        }
        ServerCmd::Add {
            name,
            address,
            password,
            modset,
        } => {
            let (host, port) = address
                .rsplit_once(':')
                .ok_or_else(|| anyhow::anyhow!("address must be host:port"))?;
            client.config.favorite_servers.retain(|s| s.name != name);
            client.config.favorite_servers.push(FavoriteServer {
                name,
                host: host.into(),
                port: port.parse()?,
                password,
                modset,
                repository: None,
            });
            client
                .config
                .favorite_servers
                .sort_by_key(|s| s.name.to_lowercase());
            client.save_config()?;
        }
        ServerCmd::Remove { name } => {
            client.config.favorite_servers.retain(|s| s.name != name);
            client.save_config()?;
        }
    }
    Ok(exit::OK)
}

pub fn server_helper(a: ServerHelperArgs, ctx: &Ctx) -> anyhow::Result<i32> {
    let client = ctx.client()?;
    let p = match &a.profile {
        Some(n) => client.profile(n),
        None => client.active_profile(),
    };
    let addons = client.addons(&p);
    let target = a
        .target
        .clone()
        .unwrap_or_else(|| a.server_dir.join("mods"));
    let mode = if a.copy || a.lowercase {
        axs_launcher::tools::LinkMode::Copy
    } else {
        axs_launcher::tools::LinkMode::Link
    };
    let setup = axs_launcher::tools::prepare_server(
        &addons,
        &names(&a.mods),
        &names(&a.server_mods),
        &a.server_dir,
        &target,
        mode,
        a.lowercase,
    )?;
    if ctx.json {
        output::json_line(&serde_json::to_value(&setup)?);
    } else {
        println!(
            "{} mod(s) in {}, {} key(s) in {}",
            setup.mods.len() + setup.server_mods.len(),
            target.display(),
            setup.keys.len(),
            a.server_dir.join("keys").display()
        );
        println!("parameters: {}", setup.parameters.join(" "));
    }
    Ok(exit::OK)
}

pub fn rpt(a: RptArgs, _ctx: &Ctx) -> anyhow::Result<i32> {
    let f = match a.file {
        Some(f) => f,
        None => axs_launcher::detect::latest_rpt(a.proton)
            .ok_or_else(|| anyhow::anyhow!("no RPT file found"))?,
    };
    eprintln!("== {}", f.display());
    let follow = a.follow;
    axs_launcher::tools::tail(&f, true, |text| {
        print!("{text}");
        follow
    })?;
    Ok(exit::OK)
}
