//! Launcher integration: profiles, addons, modsets and launching.

use std::collections::BTreeMap;

use axs_launcher::{Addon, LaunchCommand, Profile, ProfileStore};

use crate::{Client, ClientError, Result};

impl Client {
    pub fn profiles(&self) -> ProfileStore {
        ProfileStore::new(self.dirs.profiles())
    }

    /// The active profile (with a detected game executable if unset).
    pub fn active_profile(&self) -> Profile {
        let name = self
            .config
            .profile
            .clone()
            .unwrap_or_else(|| "Default".into());
        self.profile(&name)
    }

    pub fn profile(&self, name: &str) -> Profile {
        let mut p = self.profiles().get(name).unwrap_or_else(|| Profile {
            name: name.to_owned(),
            ..Default::default()
        });
        if p.executable.is_none() {
            p.executable = axs_launcher::detect::arma3_dir()
                .and_then(|d| axs_launcher::detect::default_executable(&d));
        }
        p
    }

    /// Available addons: profile search folders, game folder, Workshop and
    /// repository destinations.
    pub fn addons(&self, p: &Profile) -> BTreeMap<String, Addon> {
        let mut opts = axs_launcher::discover_for(p);
        for r in &self.repos {
            if let Some(d) = &r.destination {
                if !opts.search_dirs.contains(d) {
                    opts.search_dirs.push(d.clone());
                }
            }
        }
        axs_launcher::discover(&opts)
    }

    pub fn favorite_server(&self, name: &str) -> Option<&axs_launcher::FavoriteServer> {
        self.config
            .favorite_servers
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn launch_command(&self, p: &Profile, server: Option<&str>) -> Result<LaunchCommand> {
        let addons = self.addons(p);
        let srv =
            match server.or(p.server.as_deref()).filter(|s| !s.is_empty()) {
                Some(n) => Some(self.favorite_server(n).ok_or_else(|| {
                    ClientError::Invalid(format!("favorite server {n:?} not found"))
                })?),
                None => None,
            };
        axs_launcher::command(p, &addons, srv).map_err(|e| ClientError::Invalid(e.to_string()))
    }

    /// Add or refresh the modset group of a repository (or one of its
    /// events) in a profile.
    pub async fn apply_modset(
        &mut self,
        profile: &str,
        repo: &str,
        event: Option<&str>,
    ) -> Result<Profile> {
        let r = self.repo(repo)?.clone();
        let t = self.transport(&r)?;
        let snap = axs_sync::fetch_snapshot(&t, r.prefer_modern).await?;
        let group = match event {
            Some(ev) => {
                let e = snap
                    .manifest
                    .events
                    .iter()
                    .find(|e| e.name.eq_ignore_ascii_case(ev))
                    .ok_or_else(|| ClientError::Invalid(format!("event {ev:?} not found")))?;
                axs_launcher::event_modset(&r.name, e)
            }
            None => axs_launcher::repository_modset(&r.name, &snap.manifest),
        };
        let mut p = self.profile(profile);
        axs_launcher::upsert_modset(&mut p.groups, group);
        self.profiles().save(&p)?;
        // Servers advertised by the repository become favorites.
        let mut changed = false;
        for s in &snap.manifest.servers {
            if !self
                .config
                .favorite_servers
                .iter()
                .any(|f| f.name == s.name)
            {
                self.config
                    .favorite_servers
                    .push(axs_launcher::FavoriteServer {
                        name: s.name.clone(),
                        host: s.host.clone(),
                        port: s.port,
                        password: s.password.clone(),
                        modset: s.modset.clone(),
                        repository: Some(r.name.clone()),
                    });
                changed = true;
            }
        }
        if changed {
            self.save_config()?;
        }
        Ok(p)
    }
}
