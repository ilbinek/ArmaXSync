//! Arma 3 launcher: addon discovery, profiles/groups, run parameters and
//! process launching (direct, BattlEye, Steam, Proton), plus tools.

pub mod addons;
pub mod detect;
pub mod launch;
pub mod params;
pub mod profile;
pub mod tools;

pub use addons::{discover, Addon, AddonSource, DiscoverOptions};
pub use launch::{command, launch, start_external_apps, LaunchCommand};
pub use params::{check, load_order, run_parameters, LaunchCheck};
pub use profile::{
    ExternalApp, FavoriteServer, GroupNode, LaunchMode, LauncherOptions, ModsetRef, Profile,
    ProfileStore,
};

#[derive(Debug, thiserror::Error)]
pub enum LauncherError {
    #[error("ArmA 3 executable location is wrong or missing")]
    ExecutableMissing,
    #[error("Steam was not found")]
    SteamNotFound,
    #[error("could not start the game: {0}")]
    Spawn(String),
}

/// Discovery options for a profile (search folders, game folder, Workshop).
pub fn discover_for(p: &Profile) -> DiscoverOptions {
    let mut search = p.addon_search_dirs.clone();
    if let Some(g) = p.game_dir() {
        if !search.iter().any(|s| s == g) {
            search.push(g.to_path_buf());
        }
    }
    DiscoverOptions {
        search_dirs: search,
        game_dir: p.game_dir().map(|g| g.to_path_buf()),
        workshop_dirs: if p.use_workshop {
            detect::workshop_dirs()
        } else {
            vec![]
        },
        max_depth: 6,
    }
}

/// Groups for a repository modset: one leaf per addon folder of the repo.
pub fn repository_modset(repo: &str, manifest: &axs_model::Manifest) -> GroupNode {
    GroupNode::Group {
        name: repo.to_owned(),
        modset: Some(ModsetRef::Repository {
            repository: repo.to_owned(),
        }),
        children: manifest
            .addons
            .iter()
            .map(|a| GroupNode::Addon {
                name: axs_model::paths::file_name(a).to_owned(),
                selected: true,
                optional: false,
            })
            .collect(),
    }
}

/// Groups for an event modset (optional addons start unselected).
pub fn event_modset(repo: &str, ev: &axs_model::EventDef) -> GroupNode {
    GroupNode::Group {
        name: ev.name.clone(),
        modset: Some(ModsetRef::Event {
            repository: repo.to_owned(),
            event: ev.name.clone(),
        }),
        children: ev
            .addons
            .iter()
            .map(|a| GroupNode::Addon {
                name: a.name.clone(),
                selected: !a.optional,
                optional: a.optional,
            })
            .collect(),
    }
}

/// Replace a modset group (keeping the previous selection of known addons).
pub fn upsert_modset(groups: &mut Vec<GroupNode>, mut new: GroupNode) {
    let key = match &new {
        GroupNode::Group {
            modset: Some(m), ..
        } => m.clone(),
        _ => {
            groups.push(new);
            return;
        }
    };
    if let Some(pos) = groups
        .iter()
        .position(|g| matches!(g, GroupNode::Group { modset: Some(m), .. } if *m == key))
    {
        if let (
            GroupNode::Group { children: old, .. },
            GroupNode::Group {
                children: fresh, ..
            },
        ) = (&groups[pos], &mut new)
        {
            for c in fresh.iter_mut() {
                if let GroupNode::Addon { name, selected, .. } = c {
                    if let Some(GroupNode::Addon { selected: was, .. }) =
                        old.iter().find(|o| o.name().eq_ignore_ascii_case(name))
                    {
                        *selected = *was;
                    }
                }
            }
        }
        groups[pos] = new;
    } else {
        groups.push(new);
    }
}
