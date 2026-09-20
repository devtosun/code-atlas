#![forbid(unsafe_code)]

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use thiserror::Error;

pub const DEFAULT_RESULT_LIMIT: usize = 20;
pub const MAX_RESULT_LIMIT: usize = 200;
pub const DEFAULT_GRAPH_DEPTH: usize = 2;
pub const MAX_GRAPH_DEPTH: usize = 8;
pub const MAX_GRAPH_NODES: usize = 500;
pub const MAX_RESPONSE_BYTES: usize = 65_536;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{kind} must be 1..={max} ASCII letters, digits, '.', '_' or '-'")]
    InvalidId { kind: &'static str, max: usize },
    #[error("byte range end {end} is before start {start}")]
    InvalidByteRange { start: u64, end: u64 },
    #[error("{kind} {value} is outside the allowed range {min}..={max}")]
    LimitOutOfRange {
        kind: &'static str,
        value: usize,
        min: usize,
        max: usize,
    },
    #[error("operation cancelled")]
    Cancelled,
    #[error("repository root must be absolute: {path}")]
    RootNotAbsolute { path: PathBuf },
    #[error("repository root must be valid UTF-8: {path}")]
    RootNotUtf8 { path: PathBuf },
    #[error("repository root is not an accessible directory: {path}")]
    RootUnavailable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("repository root is not a directory: {path}")]
    RootNotDirectory { path: PathBuf },
}

fn validate_id(value: &str, kind: &'static str) -> Result<(), CoreError> {
    const MAX_ID_BYTES: usize = 128;
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(CoreError::InvalidId {
            kind,
            max: MAX_ID_BYTES,
        })
    }
}

macro_rules! validated_id {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
                let value = value.into();
                validate_id(&value, $kind)?;
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

validated_id!(RepositoryId, "repository ID");
validated_id!(GenerationId, "generation ID");
validated_id!(JobId, "job ID");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    pub fn new(start: u64, end: u64) -> Result<Self, CoreError> {
        if end < start {
            return Err(CoreError::InvalidByteRange { start, end });
        }
        Ok(Self { start, end })
    }

    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }

    #[must_use]
    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

macro_rules! bounded_limit {
    ($name:ident, $kind:literal, $min:expr, $max:expr, $default:expr) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name(usize);

        impl $name {
            pub fn new(value: usize) -> Result<Self, CoreError> {
                if ($min..=$max).contains(&value) {
                    Ok(Self(value))
                } else {
                    Err(CoreError::LimitOutOfRange {
                        kind: $kind,
                        value,
                        min: $min,
                        max: $max,
                    })
                }
            }

            #[must_use]
            pub const fn get(self) -> usize {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self($default)
            }
        }
    };
}

bounded_limit!(
    ResultLimit,
    "result limit",
    1,
    MAX_RESULT_LIMIT,
    DEFAULT_RESULT_LIMIT
);
bounded_limit!(
    GraphDepth,
    "graph depth",
    0,
    MAX_GRAPH_DEPTH,
    DEFAULT_GRAPH_DEPTH
);
bounded_limit!(
    GraphNodeLimit,
    "graph node limit",
    1,
    MAX_GRAPH_NODES,
    MAX_GRAPH_NODES
);
bounded_limit!(
    ResponseByteLimit,
    "response byte limit",
    1,
    MAX_RESPONSE_BYTES,
    MAX_RESPONSE_BYTES
);

#[derive(Clone, Debug, Default)]
pub struct CancellationContext {
    cancelled: Arc<AtomicBool>,
}

impl CancellationContext {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn check(&self) -> Result<(), CoreError> {
        if self.is_cancelled() {
            Err(CoreError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryRoot {
    path: PathBuf,
    utf8: String,
}

impl RepositoryRoot {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, CoreError> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(CoreError::RootNotAbsolute { path });
        }
        path.to_str()
            .ok_or_else(|| CoreError::RootNotUtf8 { path: path.clone() })?;
        let path = std::fs::canonicalize(&path).map_err(|source| CoreError::RootUnavailable {
            path: path.clone(),
            source,
        })?;
        let utf8 = path
            .to_str()
            .ok_or_else(|| CoreError::RootNotUtf8 { path: path.clone() })?
            .to_owned();
        let metadata = std::fs::metadata(&path).map_err(|source| CoreError::RootUnavailable {
            path: path.clone(),
            source,
        })?;
        if !metadata.is_dir() {
            return Err(CoreError::RootNotDirectory { path });
        }
        Ok(Self { path, utf8 })
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn as_utf8(&self) -> &str {
        &self.utf8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryLifecycle {
    NotOpened,
    Opening,
    Ready,
    ReadOnlyFollower,
    Degraded,
}

impl RepositoryLifecycle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotOpened => "not_opened",
            Self::Opening => "opening",
            Self::Ready => "ready",
            Self::ReadOnlyFollower => "read_only_follower",
            Self::Degraded => "degraded",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessState {
    lifecycle: RepositoryLifecycle,
    repository_id: Option<RepositoryId>,
    generation_id: Option<GenerationId>,
}

impl ProcessState {
    #[must_use]
    pub const fn not_opened() -> Self {
        Self {
            lifecycle: RepositoryLifecycle::NotOpened,
            repository_id: None,
            generation_id: None,
        }
    }

    #[must_use]
    pub const fn lifecycle(&self) -> RepositoryLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub fn repository_id(&self) -> Option<&RepositoryId> {
        self.repository_id.as_ref()
    }

    #[must_use]
    pub fn generation_id(&self) -> Option<&GenerationId> {
        self.generation_id.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_reject_ambiguous_or_unbounded_values() {
        assert!(RepositoryId::new("repo-1.main").is_ok());
        assert!(RepositoryId::new("").is_err());
        assert!(RepositoryId::new("contains/slash").is_err());
        assert!(GenerationId::new("x".repeat(129)).is_err());
        assert!(JobId::new("job-0001").is_ok());
    }

    #[test]
    fn byte_ranges_are_half_open_and_ordered() {
        let range = ByteRange::new(4, 9).expect("valid constant range");
        assert_eq!(range.len(), 5);
        assert!(!range.is_empty());
        assert!(ByteRange::new(9, 4).is_err());
    }

    #[test]
    fn generated_identifiers_and_ranges_match_their_validation_properties() {
        let alphabet = b"aZ09._-/\0";
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for length in 0..=140 {
            let mut candidate = String::with_capacity(length);
            for _ in 0..length {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                candidate.push(char::from(alphabet[(state as usize) % alphabet.len()]));
            }
            let expected = !candidate.is_empty()
                && candidate.len() <= 128
                && candidate
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
            assert_eq!(
                RepositoryId::new(candidate.clone()).is_ok(),
                expected,
                "identifier property differed for {candidate:?}"
            );
            assert_eq!(GenerationId::new(candidate.clone()).is_ok(), expected);
            assert_eq!(JobId::new(candidate).is_ok(), expected);
        }

        for _ in 0..10_000 {
            state = state
                .wrapping_mul(2_862_933_555_777_941_757)
                .wrapping_add(3_037_000_493);
            let start = state;
            state = state
                .wrapping_mul(2_862_933_555_777_941_757)
                .wrapping_add(3_037_000_493);
            let end = state;
            assert_eq!(ByteRange::new(start, end).is_ok(), end >= start);
        }
    }

    #[test]
    fn limits_reject_values_outside_policy() {
        assert_eq!(ResultLimit::default().get(), DEFAULT_RESULT_LIMIT);
        assert!(ResultLimit::new(0).is_err());
        assert!(ResultLimit::new(MAX_RESULT_LIMIT + 1).is_err());
        assert!(GraphDepth::new(0).is_ok());
        assert!(ResponseByteLimit::new(MAX_RESPONSE_BYTES + 1).is_err());
    }

    #[test]
    fn cancellation_is_shared_and_cooperative() {
        let original = CancellationContext::default();
        let worker = original.clone();
        assert!(worker.check().is_ok());
        original.cancel();
        assert!(worker.is_cancelled());
        assert!(matches!(worker.check(), Err(CoreError::Cancelled)));
    }

    #[test]
    fn repository_root_must_be_absolute_and_existing() {
        assert!(RepositoryRoot::new("relative").is_err());
        let current = std::env::current_dir().expect("test process has a current directory");
        assert_eq!(
            RepositoryRoot::new(&current)
                .expect("current directory is a valid root")
                .as_path(),
            current
        );
    }

    #[cfg(unix)]
    #[test]
    fn repository_root_rejects_non_utf8_paths_before_access() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let path = PathBuf::from("/tmp").join(OsString::from_vec(vec![0xff]));
        assert!(matches!(
            RepositoryRoot::new(path),
            Err(CoreError::RootNotUtf8 { .. })
        ));
    }
}
