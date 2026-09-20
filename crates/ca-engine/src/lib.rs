#![forbid(unsafe_code)]

use std::path::Path;

use ca_core::{ProcessState, RepositoryRoot};

pub mod indexing;
pub mod memory;
pub mod repository;
pub mod resolution;
pub mod retrieval;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryStatus {
    root: RepositoryRoot,
    state: ProcessState,
}

impl RepositoryStatus {
    #[must_use]
    pub fn root(&self) -> &Path {
        self.root.as_path()
    }

    #[must_use]
    pub fn root_utf8(&self) -> &str {
        self.root.as_utf8()
    }

    #[must_use]
    pub fn state(&self) -> &ProcessState {
        &self.state
    }
}

#[derive(Clone, Debug)]
pub struct RepositoryStatusService {
    root: RepositoryRoot,
    state: ProcessState,
}

impl RepositoryStatusService {
    #[must_use]
    pub const fn new(root: RepositoryRoot) -> Self {
        Self {
            root,
            state: ProcessState::not_opened(),
        }
    }

    #[must_use]
    pub fn status(&self) -> RepositoryStatus {
        RepositoryStatus {
            root: self.root.clone(),
            state: self.state.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use ca_core::RepositoryLifecycle;

    use super::*;

    #[test]
    fn status_is_not_opened_until_explicit_repository_work_begins() {
        let root = RepositoryRoot::new(
            std::env::current_dir().expect("test process has a current directory"),
        )
        .expect("current directory is a valid repository root");
        let status = RepositoryStatusService::new(root).status();
        assert_eq!(status.state().lifecycle(), RepositoryLifecycle::NotOpened);
        assert!(status.state().repository_id().is_none());
        assert!(status.state().generation_id().is_none());
    }
}
