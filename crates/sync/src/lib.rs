//! Synchronizing a local folder with a remote repository.
//!
//! 1. [`fetch_snapshot`] loads the remote description (modern manifest or
//!    legacy `.a3s` files, normalized to a [`axs_model::Manifest`]);
//! 2. [`snapshot_local`] inspects the destination;
//! 3. [`plan`] computes a [`SyncPlan`] (pure, serializable for previews);
//! 4. [`execute`] applies it.

mod exec;
mod plan;
mod remote;
mod status;

pub use exec::{execute, EventFn, ExecOptions, ExecReport, Method, SyncEvent};
pub use plan::{plan, snapshot_local, LocalEntry, LocalSnapshot, Op, PlanOptions, SyncPlan};
pub use remote::{
    fetch_autoconfig, fetch_legacy, fetch_snapshot, legacy_to_manifest, RemoteSnapshot, SourceKind,
};
pub use status::{event_selection, repo_status, RepoStatus};

pub use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Transport(#[from] axs_transport::TransportError),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Remote(String),
    #[error("{path}: checksum mismatch (expected {expected}, got {actual})")]
    Verify {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("{0}: incomplete transfer")]
    Incomplete(String),
    #[error("cancelled")]
    Cancelled,
}

impl SyncError {
    pub fn is_retryable(&self) -> bool {
        match self {
            SyncError::Transport(t) => t.is_retryable(),
            SyncError::Incomplete(_) => true,
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, SyncError>;
