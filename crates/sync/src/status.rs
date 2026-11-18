//! Repository update status and event selections.

use serde::{Deserialize, Serialize};

use axs_model::Manifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoStatus {
    /// Local copy is at the remote revision (or later builds changed nothing).
    Ok,
    /// New content is available.
    Updated,
    /// The repository could not be checked.
    Error,
    /// Never checked.
    Unknown,
}

/// Legacy rules: same revision → OK; behind and the server reports content
/// changes (now or in any changelog after the local revision) → UPDATED.
pub fn repo_status(local_revision: Option<u32>, m: &Manifest) -> RepoStatus {
    let Some(local) = local_revision else {
        return RepoStatus::Updated;
    };
    if local >= m.revision {
        return RepoStatus::Ok;
    }
    if m.content_updated {
        return RepoStatus::Updated;
    }
    if m.changelogs
        .iter()
        .any(|c| c.revision > local && c.content_updated)
    {
        return RepoStatus::Updated;
    }
    // Changelogs only keep 10 entries: if we are older than all of them we
    // cannot know, so assume an update.
    if m.changelogs
        .first()
        .map(|c| c.revision > local + 1)
        .unwrap_or(true)
    {
        return RepoStatus::Updated;
    }
    RepoStatus::Ok
}

/// Repository sub-paths selected by an event: its addon folders (wherever
/// they are in the tree) and its `userconfig/<name>` folders.
pub fn event_selection(m: &Manifest, event: &str) -> Option<Vec<String>> {
    let ev = m.events.iter().find(|e| e.name == event)?;
    let mut sel = vec![];
    for a in &ev.addons {
        for path in &m.addons {
            if axs_model::paths::file_name(path).eq_ignore_ascii_case(&a.name) {
                sel.push(path.clone());
            }
        }
    }
    for u in &ev.userconfig {
        sel.push(format!("userconfig/{}", u.name));
    }
    Some(sel)
}
