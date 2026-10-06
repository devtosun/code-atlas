use std::{
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

use ca_core::{CancellationContext, RepositoryId, RepositoryRoot};
use ignore::{Match, gitignore::Gitignore};
use thiserror::Error;

pub const DEFAULT_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const DEFAULT_MAX_FILES: usize = 100_000;
pub const DEFAULT_MAX_DIRECTORIES: usize = 100_000;
pub const MAX_RELATIVE_PATH_BYTES: usize = 4_096;
pub const MAX_PATH_SEGMENTS: usize = 256;
const MAX_SEGMENT_BYTES: usize = 255;
const IGNORE_FILENAMES: &[&str] = &[".gitignore", ".ignore", ".codeatlasignore"];

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("invalid relative source path '{path}': {reason}")]
    InvalidRelativePath { path: String, reason: &'static str },
    #[error("source path is excluded by policy: {path}")]
    Excluded { path: String },
    #[error("source path resolves outside the authorized root: {path}")]
    OutOfRoot { path: String },
    #[error("symbolic links and junction-like entries are not followed: {path}")]
    LinkNotAllowed { path: String },
    #[error("source path is not a regular file: {path}")]
    NotAFile { path: String },
    #[error("source file exceeds the {limit} byte limit: {path}")]
    FileTooLarge { path: String, limit: u64 },
    #[error("binary source is not accepted: {path}")]
    BinaryFile { path: String },
    #[error("source is not valid UTF-8: {path}")]
    InvalidUtf8 { path: String },
    #[error("source changed while it was being opened: {path}")]
    ChangedDuringRead { path: String },
    #[error("cannot {operation} '{path}': {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid ignore rule in '{path}': {message}")]
    InvalidIgnoreRule { path: String, message: String },
    #[error("Git metadata at the authorized root is invalid: {0}")]
    GitMetadata(String),
    #[error("canonical repository path is not valid UTF-8: {0}")]
    NonUtf8Root(PathBuf),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RelativeSourcePath {
    normalized: String,
    platform: PathBuf,
}

impl RelativeSourcePath {
    pub fn new(value: impl Into<String>) -> Result<Self, RepositoryError> {
        let value = value.into();
        if value.is_empty() {
            return Err(invalid_path(value, "path is empty"));
        }
        if value.len() > MAX_RELATIVE_PATH_BYTES {
            return Err(invalid_path(value, "path is too long"));
        }
        if value.contains('\0') {
            return Err(invalid_path(value, "NUL is not allowed"));
        }
        if value.contains('\\') {
            return Err(invalid_path(value, "backslash separators are not allowed"));
        }
        if looks_like_windows_absolute(&value) {
            return Err(invalid_path(
                value,
                "Windows absolute/device paths are not allowed",
            ));
        }
        let textual_segments = value.split('/').collect::<Vec<_>>();
        if textual_segments
            .iter()
            .any(|segment| segment.is_empty() || matches!(*segment, "." | ".."))
        {
            return Err(invalid_path(
                value,
                "empty, '.' and '..' path components are not allowed",
            ));
        }
        let platform = PathBuf::from(&value);
        let mut segments = 0_usize;
        for component in platform.components() {
            match component {
                Component::Normal(segment) => {
                    let segment = segment
                        .to_str()
                        .ok_or_else(|| invalid_path(value.clone(), "path segment is not UTF-8"))?;
                    if segment.len() > MAX_SEGMENT_BYTES {
                        return Err(invalid_path(value, "path segment is too long"));
                    }
                    segments += 1;
                }
                Component::CurDir => {
                    return Err(invalid_path(value, "'.' components are not allowed"));
                }
                Component::ParentDir => {
                    return Err(invalid_path(value, "'..' components are not allowed"));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(invalid_path(value, "absolute paths are not allowed"));
                }
            }
        }
        if segments == 0 || segments > MAX_PATH_SEGMENTS {
            return Err(invalid_path(value, "invalid number of path segments"));
        }
        Ok(Self {
            normalized: value,
            platform,
        })
    }

    pub fn from_path(path: &Path) -> Result<Self, RepositoryError> {
        let mut normalized = String::new();
        for component in path.components() {
            let Component::Normal(segment) = component else {
                return Err(invalid_path(
                    path.display().to_string(),
                    "scanner produced a non-relative path",
                ));
            };
            let segment = segment.to_str().ok_or_else(|| {
                invalid_path(path.display().to_string(), "path segment is not UTF-8")
            })?;
            if !normalized.is_empty() {
                normalized.push('/');
            }
            normalized.push_str(segment);
        }
        Self::new(normalized)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.normalized
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.platform
    }
}

fn invalid_path(path: String, reason: &'static str) -> RepositoryError {
    RepositoryError::InvalidRelativePath { path, reason }
}

fn looks_like_windows_absolute(value: &str) -> bool {
    value.starts_with("//")
        || value.starts_with("\\\\")
        || value.as_bytes().get(1) == Some(&b':')
        || value.starts_with("//?/")
        || value.starts_with("//./")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryKind {
    Gitless,
    MainWorktree,
    LinkedWorktree,
}

impl RepositoryKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gitless => "gitless",
            Self::MainWorktree => "main_worktree",
            Self::LinkedWorktree => "linked_worktree",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryIdentity {
    id: RepositoryId,
    root: RepositoryRoot,
    kind: RepositoryKind,
    git_dir: Option<PathBuf>,
    common_git_dir: Option<PathBuf>,
}

impl RepositoryIdentity {
    pub fn derive(root: RepositoryRoot) -> Result<Self, RepositoryError> {
        let git_marker = root.as_path().join(".git");
        let git_marker_exists = match fs::symlink_metadata(&git_marker) {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(source) => {
                return Err(RepositoryError::Io {
                    operation: "inspect Git marker",
                    path: git_marker.clone(),
                    source,
                });
            }
        };
        let opened = gix::open_opts(
            root.as_path(),
            gix::open::Options::isolated().strict_config(true),
        );
        let (kind, git_dir, common_git_dir) = match opened {
            Ok(repository) => {
                let git_dir = fs::canonicalize(repository.git_dir()).map_err(|source| {
                    RepositoryError::Io {
                        operation: "canonicalize Git directory",
                        path: repository.git_dir().to_owned(),
                        source,
                    }
                })?;
                let common_git_dir =
                    fs::canonicalize(repository.common_dir()).map_err(|source| {
                        RepositoryError::Io {
                            operation: "canonicalize common Git directory",
                            path: repository.common_dir().to_owned(),
                            source,
                        }
                    })?;
                let kind = if git_dir == common_git_dir {
                    RepositoryKind::MainWorktree
                } else {
                    RepositoryKind::LinkedWorktree
                };
                (kind, Some(git_dir), Some(common_git_dir))
            }
            Err(gix::open::Error::NotARepository { .. }) if !git_marker_exists => {
                (RepositoryKind::Gitless, None, None)
            }
            Err(error) => return Err(RepositoryError::GitMetadata(error.to_string())),
        };

        let mut hasher = blake3::Hasher::new();
        hasher.update(b"codeatlas-worktree-v1\0");
        hasher.update(root.as_utf8().as_bytes());
        if let Some(path) = &git_dir {
            hasher.update(b"\0git-dir\0");
            hasher.update(path_utf8(path)?.as_bytes());
        }
        if let Some(path) = &common_git_dir {
            hasher.update(b"\0common-dir\0");
            hasher.update(path_utf8(path)?.as_bytes());
        }
        let id = RepositoryId::new(format!("wt-{}", hasher.finalize().to_hex()))
            .map_err(|error| RepositoryError::GitMetadata(error.to_string()))?;
        Ok(Self {
            id,
            root,
            kind,
            git_dir,
            common_git_dir,
        })
    }

    #[must_use]
    pub fn id(&self) -> &RepositoryId {
        &self.id
    }

    #[must_use]
    pub fn root(&self) -> &RepositoryRoot {
        &self.root
    }

    #[must_use]
    pub const fn kind(&self) -> RepositoryKind {
        self.kind
    }

    #[must_use]
    pub fn git_dir(&self) -> Option<&Path> {
        self.git_dir.as_deref()
    }

    #[must_use]
    pub fn common_git_dir(&self) -> Option<&Path> {
        self.common_git_dir.as_deref()
    }
}

fn path_utf8(path: &Path) -> Result<&str, RepositoryError> {
    path.to_str()
        .ok_or_else(|| RepositoryError::NonUtf8Root(path.to_owned()))
}

#[derive(Clone, Debug)]
pub struct ScanPolicy {
    max_file_bytes: u64,
    max_files: usize,
    max_directories: usize,
    explicit_excludes: Vec<RelativeSourcePath>,
}

impl Default for ScanPolicy {
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_files: DEFAULT_MAX_FILES,
            max_directories: DEFAULT_MAX_DIRECTORIES,
            explicit_excludes: Vec::new(),
        }
    }
}

impl ScanPolicy {
    pub fn new(
        max_file_bytes: u64,
        max_files: usize,
        max_directories: usize,
        explicit_excludes: impl IntoIterator<Item = String>,
    ) -> Result<Self, RepositoryError> {
        if max_file_bytes == 0 || max_files == 0 || max_directories == 0 {
            return Err(invalid_path(
                "scan policy".to_owned(),
                "limits must be greater than zero",
            ));
        }
        let explicit_excludes = explicit_excludes
            .into_iter()
            .map(RelativeSourcePath::new)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            max_file_bytes,
            max_files,
            max_directories,
            explicit_excludes,
        })
    }

    fn is_excluded(&self, relative: &RelativeSourcePath) -> bool {
        hard_denied(relative)
            || self.explicit_excludes.iter().any(|excluded| {
                relative.as_path() == excluded.as_path()
                    || relative.as_path().starts_with(excluded.as_path())
            })
    }
}

fn hard_denied(relative: &RelativeSourcePath) -> bool {
    const DENIED_DIRECTORIES: &[&str] = &[
        ".git",
        ".dart_tool",
        ".gradle",
        "node_modules",
        "target",
        "bin",
        "obj",
        "build",
        "dist",
        "vendor",
    ];
    if relative.as_path().components().any(|component| {
        let Component::Normal(name) = component else {
            return true;
        };
        DENIED_DIRECTORIES
            .iter()
            .any(|denied| name.eq_ignore_ascii_case(denied))
    }) {
        return true;
    }
    let Some(file_name) = relative
        .as_path()
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return true;
    };
    let lower = file_name.to_ascii_lowercase();
    lower == ".env"
        || lower.starts_with(".env.")
        || matches!(
            lower.as_str(),
            "credentials" | "credentials.json" | "id_rsa" | "id_dsa" | "id_ed25519"
        )
        || [".pem", ".key", ".p12", ".pfx", ".crt", ".cer"]
            .iter()
            .any(|extension| lower.ends_with(extension))
        || [".min.js", ".min.mjs", ".min.cjs"]
            .iter()
            .any(|extension| lower.ends_with(extension))
}

fn supported_source(relative: &RelativeSourcePath) -> bool {
    let Some(extension) = relative
        .as_path()
        .extension()
        .and_then(|extension| extension.to_str())
    else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "dart"
            | "cs"
            | "rs"
            | "go"
            | "java"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "ts"
            | "tsx"
            | "mts"
            | "cts"
    )
}

const SUPPORTED_ROOT_MANIFESTS: [&str; 2] = ["go.mod", "pubspec.yaml"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFile {
    relative_path: RelativeSourcePath,
    text: String,
    content_hash: String,
}

impl SourceFile {
    #[must_use]
    pub fn relative_path(&self) -> &RelativeSourcePath {
        &self.relative_path
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

#[derive(Clone, Debug)]
pub struct SourceReader {
    identity: RepositoryIdentity,
    policy: ScanPolicy,
}

impl SourceReader {
    #[must_use]
    pub const fn new(identity: RepositoryIdentity, policy: ScanPolicy) -> Self {
        Self { identity, policy }
    }

    pub fn read(&self, relative: &RelativeSourcePath) -> Result<SourceFile, RepositoryError> {
        if !supported_source(relative) {
            return Err(RepositoryError::Excluded {
                path: relative.as_str().to_owned(),
            });
        }
        if self.is_ignored(relative)? {
            return Err(RepositoryError::Excluded {
                path: relative.as_str().to_owned(),
            });
        }
        self.read_internal(relative, || {})
    }

    /// Reads the small, explicit manifest allowlist as inert UTF-8 data. This never
    /// invokes a package manager, build tool, script, or repository configuration.
    pub fn read_supported_root_manifests(&self) -> Result<Vec<SourceFile>, RepositoryError> {
        let mut manifests = Vec::new();
        for name in SUPPORTED_ROOT_MANIFESTS {
            let relative = RelativeSourcePath::new(name)?;
            let absolute = self.identity.root().as_path().join(name);
            match fs::symlink_metadata(&absolute) {
                Ok(metadata) if metadata.is_file() && !is_link_like(&metadata) => {
                    if !self.is_ignored(&relative)? {
                        manifests.push(self.read_internal(&relative, || {})?);
                    }
                }
                Ok(metadata) if is_link_like(&metadata) => {
                    return Err(RepositoryError::LinkNotAllowed {
                        path: name.to_owned(),
                    });
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(RepositoryError::Io {
                        operation: "inspect supported manifest",
                        path: absolute,
                        source,
                    });
                }
            }
        }
        Ok(manifests)
    }

    fn read_internal(
        &self,
        relative: &RelativeSourcePath,
        after_metadata: impl FnOnce(),
    ) -> Result<SourceFile, RepositoryError> {
        if self.policy.is_excluded(relative) {
            return Err(RepositoryError::Excluded {
                path: relative.as_str().to_owned(),
            });
        }
        let absolute = self.identity.root().as_path().join(relative.as_path());
        self.validate_components(relative)?;
        let before = fs::symlink_metadata(&absolute).map_err(|source| RepositoryError::Io {
            operation: "inspect source",
            path: absolute.clone(),
            source,
        })?;
        if is_link_like(&before) {
            return Err(RepositoryError::LinkNotAllowed {
                path: relative.as_str().to_owned(),
            });
        }
        if !before.is_file() {
            return Err(RepositoryError::NotAFile {
                path: relative.as_str().to_owned(),
            });
        }
        if before.len() > self.policy.max_file_bytes {
            return Err(RepositoryError::FileTooLarge {
                path: relative.as_str().to_owned(),
                limit: self.policy.max_file_bytes,
            });
        }

        after_metadata();
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let mut file = options
            .open(&absolute)
            .map_err(|source| RepositoryError::Io {
                operation: "open source without following links",
                path: absolute.clone(),
                source,
            })?;
        let opened = file.metadata().map_err(|source| RepositoryError::Io {
            operation: "inspect opened source",
            path: absolute.clone(),
            source,
        })?;
        if !same_file(&before, &opened) {
            return Err(RepositoryError::ChangedDuringRead {
                path: relative.as_str().to_owned(),
            });
        }
        let canonical = fs::canonicalize(&absolute).map_err(|source| RepositoryError::Io {
            operation: "canonicalize opened source",
            path: absolute.clone(),
            source,
        })?;
        if !canonical.starts_with(self.identity.root().as_path()) {
            return Err(RepositoryError::OutOfRoot {
                path: relative.as_str().to_owned(),
            });
        }

        let read_limit = self.policy.max_file_bytes.saturating_add(1);
        let capacity = usize::try_from(before.len().min(self.policy.max_file_bytes)).unwrap_or(0);
        let mut bytes = Vec::with_capacity(capacity);
        file.by_ref()
            .take(read_limit)
            .read_to_end(&mut bytes)
            .map_err(|source| RepositoryError::Io {
                operation: "read source",
                path: absolute.clone(),
                source,
            })?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > self.policy.max_file_bytes {
            return Err(RepositoryError::FileTooLarge {
                path: relative.as_str().to_owned(),
                limit: self.policy.max_file_bytes,
            });
        }
        let after = file.metadata().map_err(|source| RepositoryError::Io {
            operation: "reinspect opened source",
            path: absolute,
            source,
        })?;
        if !same_file(&opened, &after)
            || after.len() != u64::try_from(bytes.len()).unwrap_or(u64::MAX)
        {
            return Err(RepositoryError::ChangedDuringRead {
                path: relative.as_str().to_owned(),
            });
        }
        if bytes.contains(&0) {
            return Err(RepositoryError::BinaryFile {
                path: relative.as_str().to_owned(),
            });
        }
        let text = String::from_utf8(bytes).map_err(|_| RepositoryError::InvalidUtf8 {
            path: relative.as_str().to_owned(),
        })?;
        let content_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
        Ok(SourceFile {
            relative_path: relative.clone(),
            text,
            content_hash,
        })
    }

    fn validate_components(&self, relative: &RelativeSourcePath) -> Result<(), RepositoryError> {
        let mut cursor = self.identity.root().as_path().to_owned();
        for component in relative.as_path().components() {
            let Component::Normal(segment) = component else {
                return Err(RepositoryError::OutOfRoot {
                    path: relative.as_str().to_owned(),
                });
            };
            cursor.push(segment);
            let metadata = fs::symlink_metadata(&cursor).map_err(|source| RepositoryError::Io {
                operation: "inspect path component",
                path: cursor.clone(),
                source,
            })?;
            if is_link_like(&metadata) {
                return Err(RepositoryError::LinkNotAllowed {
                    path: relative.as_str().to_owned(),
                });
            }
        }
        Ok(())
    }

    fn is_ignored(&self, relative: &RelativeSourcePath) -> Result<bool, RepositoryError> {
        let mut rules = Vec::new();
        let mut directory = PathBuf::new();
        self.load_ignore_rules(&directory, &mut rules)?;
        if let Some(parent) = relative.as_path().parent() {
            for component in parent.components() {
                let Component::Normal(segment) = component else {
                    return Err(RepositoryError::OutOfRoot {
                        path: relative.as_str().to_owned(),
                    });
                };
                directory.push(segment);
                if matches_ignore(
                    &rules,
                    &self.identity.root().as_path().join(&directory),
                    true,
                ) {
                    return Ok(true);
                }
                self.load_ignore_rules(&directory, &mut rules)?;
            }
        }
        Ok(matches_ignore(
            &rules,
            &self.identity.root().as_path().join(relative.as_path()),
            false,
        ))
    }

    fn load_ignore_rules(
        &self,
        directory: &Path,
        rules: &mut Vec<Gitignore>,
    ) -> Result<(), RepositoryError> {
        for name in IGNORE_FILENAMES {
            let relative_path = if directory.as_os_str().is_empty() {
                PathBuf::from(name)
            } else {
                directory.join(name)
            };
            let relative = RelativeSourcePath::from_path(&relative_path)?;
            let absolute = self.identity.root().as_path().join(&relative_path);
            match fs::symlink_metadata(&absolute) {
                Ok(metadata) if metadata.is_file() && !is_link_like(&metadata) => {
                    let source = self.read_internal(&relative, || {})?;
                    rules.push(build_ignore_matcher(
                        self.identity.root().as_path().join(directory),
                        &absolute,
                        source.text(),
                    )?);
                }
                Ok(metadata) if is_link_like(&metadata) => {
                    return Err(RepositoryError::LinkNotAllowed {
                        path: relative.as_str().to_owned(),
                    });
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(RepositoryError::Io {
                        operation: "inspect ignore file",
                        path: absolute,
                        source,
                    });
                }
            }
        }
        Ok(())
    }
}

fn build_ignore_matcher(
    directory: PathBuf,
    source_path: &Path,
    contents: &str,
) -> Result<Gitignore, RepositoryError> {
    let mut builder = ignore::gitignore::GitignoreBuilder::new(directory);
    for line in contents.lines() {
        builder
            .add_line(Some(source_path.to_owned()), line)
            .map_err(|error| RepositoryError::InvalidIgnoreRule {
                path: source_path.display().to_string(),
                message: error.to_string(),
            })?;
    }
    builder
        .build()
        .map_err(|error| RepositoryError::InvalidIgnoreRule {
            path: source_path.display().to_string(),
            message: error.to_string(),
        })
}

fn matches_ignore(rules: &[Gitignore], absolute: &Path, is_dir: bool) -> bool {
    let mut ignored = false;
    for rule in rules {
        match rule.matched(absolute, is_dir) {
            Match::Ignore(_) => ignored = true,
            Match::Whitelist(_) => ignored = false,
            Match::None => {}
        }
    }
    ignored
}

#[cfg(windows)]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(unix)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    left.volume_serial_number() == right.volume_serial_number()
        && left.file_index() == right.file_index()
}

#[cfg(not(any(unix, windows)))]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.len() == right.len() && left.modified().ok() == right.modified().ok()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScanDiagnosticKind {
    Traversal,
    ExcludedLink,
    TooLarge,
    Binary,
    InvalidUtf8,
    ChangedDuringRead,
    InvalidIgnore,
    LimitReached,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanDiagnostic {
    pub path: String,
    pub kind: ScanDiagnosticKind,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScannedFile {
    pub relative_path: RelativeSourcePath,
    pub byte_length: u64,
    pub content_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanResult {
    pub files: Vec<ScannedFile>,
    pub diagnostics: Vec<ScanDiagnostic>,
    pub complete: bool,
}

pub struct FileScanner {
    reader: SourceReader,
}

impl FileScanner {
    #[must_use]
    pub const fn new(reader: SourceReader) -> Self {
        Self { reader }
    }

    pub fn scan(&self) -> ScanResult {
        self.scan_with_cancellation(&CancellationContext::default())
    }

    pub fn scan_with_cancellation(&self, cancellation: &CancellationContext) -> ScanResult {
        let mut result = ScanResult {
            files: Vec::new(),
            diagnostics: Vec::new(),
            complete: true,
        };
        let mut rules = Vec::new();
        let mut directories = 0_usize;
        self.visit_directory(
            Path::new(""),
            &mut rules,
            &mut directories,
            &mut result,
            cancellation,
        );
        result
            .files
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        result
    }

    pub fn scan_paths(
        &self,
        paths: &[RelativeSourcePath],
        cancellation: &CancellationContext,
    ) -> ScanResult {
        let mut result = ScanResult {
            files: Vec::with_capacity(paths.len()),
            diagnostics: Vec::new(),
            complete: true,
        };
        for relative in paths {
            if cancellation.is_cancelled() {
                result.complete = false;
                result.diagnostics.push(ScanDiagnostic {
                    path: relative.as_str().to_owned(),
                    kind: ScanDiagnosticKind::Cancelled,
                    message: "repository scan cancelled".to_owned(),
                });
                break;
            }
            match self.reader.read(relative) {
                Ok(source) => result.files.push(ScannedFile {
                    relative_path: relative.clone(),
                    byte_length: u64::try_from(source.bytes().len()).unwrap_or(u64::MAX),
                    content_hash: source.content_hash().to_owned(),
                }),
                Err(RepositoryError::Excluded { .. }) => {}
                Err(RepositoryError::Io { source, .. })
                    if source.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    if !matches!(&error, RepositoryError::LinkNotAllowed { .. }) {
                        result.complete = false;
                    }
                    result
                        .diagnostics
                        .push(diagnostic_from_error(relative.as_str().to_owned(), error));
                }
            }
        }
        result
    }

    fn visit_directory(
        &self,
        relative_directory: &Path,
        rules: &mut Vec<Gitignore>,
        directories: &mut usize,
        result: &mut ScanResult,
        cancellation: &CancellationContext,
    ) {
        if cancellation.is_cancelled() {
            result.complete = false;
            result.diagnostics.push(ScanDiagnostic {
                path: relative_directory.display().to_string(),
                kind: ScanDiagnosticKind::Cancelled,
                message: "repository scan cancelled".to_owned(),
            });
            return;
        }
        if *directories >= self.reader.policy.max_directories {
            result.complete = false;
            result.diagnostics.push(ScanDiagnostic {
                path: relative_directory.display().to_string(),
                kind: ScanDiagnosticKind::LimitReached,
                message: "directory limit reached".to_owned(),
            });
            return;
        }
        *directories += 1;
        let original_rule_count = rules.len();
        if let Err(error) = self.reader.load_ignore_rules(relative_directory, rules) {
            result.complete = false;
            result.diagnostics.push(diagnostic_from_error(
                relative_directory.display().to_string(),
                error,
            ));
            rules.truncate(original_rule_count);
            return;
        }
        let absolute_directory = self
            .reader
            .identity
            .root()
            .as_path()
            .join(relative_directory);
        let entries = match fs::read_dir(&absolute_directory) {
            Ok(entries) => entries,
            Err(error) => {
                result.complete = false;
                result.diagnostics.push(ScanDiagnostic {
                    path: relative_directory.display().to_string(),
                    kind: ScanDiagnosticKind::Traversal,
                    message: error.to_string(),
                });
                rules.truncate(original_rule_count);
                return;
            }
        };
        let mut entries = entries
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|error| {
                result.complete = false;
                result.diagnostics.push(ScanDiagnostic {
                    path: relative_directory.display().to_string(),
                    kind: ScanDiagnosticKind::Traversal,
                    message: error.to_string(),
                });
                Vec::new()
            });
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            if cancellation.is_cancelled() {
                result.complete = false;
                result.diagnostics.push(ScanDiagnostic {
                    path: relative_directory.display().to_string(),
                    kind: ScanDiagnosticKind::Cancelled,
                    message: "repository scan cancelled".to_owned(),
                });
                break;
            }
            let path = entry.path();
            let relative_path = match path.strip_prefix(self.reader.identity.root().as_path()) {
                Ok(path) => path,
                Err(_) => {
                    result.complete = false;
                    result.diagnostics.push(ScanDiagnostic {
                        path: path.display().to_string(),
                        kind: ScanDiagnosticKind::Traversal,
                        message: "directory entry escaped the authorized root".to_owned(),
                    });
                    continue;
                }
            };
            let relative = match RelativeSourcePath::from_path(relative_path) {
                Ok(relative) => relative,
                Err(error) => {
                    result.complete = false;
                    result
                        .diagnostics
                        .push(diagnostic_from_error(path.display().to_string(), error));
                    continue;
                }
            };
            if relative
                .as_path()
                .file_name()
                .is_some_and(|name| IGNORE_FILENAMES.iter().any(|ignore| name == *ignore))
            {
                continue;
            }
            if self.reader.policy.is_excluded(&relative) {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    result.complete = false;
                    result.diagnostics.push(ScanDiagnostic {
                        path: relative.as_str().to_owned(),
                        kind: ScanDiagnosticKind::Traversal,
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            if matches_ignore(rules, &path, metadata.is_dir()) {
                continue;
            }
            if is_link_like(&metadata) {
                result.diagnostics.push(ScanDiagnostic {
                    path: relative.as_str().to_owned(),
                    kind: ScanDiagnosticKind::ExcludedLink,
                    message: "symbolic link was not followed".to_owned(),
                });
                continue;
            }
            if metadata.is_dir() {
                self.visit_directory(relative.as_path(), rules, directories, result, cancellation);
            } else if metadata.is_file() {
                if !supported_source(&relative) {
                    continue;
                }
                if result.files.len() >= self.reader.policy.max_files {
                    result.complete = false;
                    result.diagnostics.push(ScanDiagnostic {
                        path: relative.as_str().to_owned(),
                        kind: ScanDiagnosticKind::LimitReached,
                        message: "file limit reached".to_owned(),
                    });
                    break;
                }
                match self.reader.read_internal(&relative, || {}) {
                    Ok(source) => result.files.push(ScannedFile {
                        relative_path: relative,
                        byte_length: u64::try_from(source.bytes().len()).unwrap_or(u64::MAX),
                        content_hash: source.content_hash().to_owned(),
                    }),
                    Err(RepositoryError::Excluded { .. }) => {}
                    Err(error) => {
                        if !matches!(&error, RepositoryError::LinkNotAllowed { .. }) {
                            result.complete = false;
                        }
                        result
                            .diagnostics
                            .push(diagnostic_from_error(relative.as_str().to_owned(), error));
                    }
                }
            }
        }
        rules.truncate(original_rule_count);
    }
}

fn diagnostic_from_error(path: String, error: RepositoryError) -> ScanDiagnostic {
    let kind = match error {
        RepositoryError::LinkNotAllowed { .. } => ScanDiagnosticKind::ExcludedLink,
        RepositoryError::FileTooLarge { .. } => ScanDiagnosticKind::TooLarge,
        RepositoryError::BinaryFile { .. } => ScanDiagnosticKind::Binary,
        RepositoryError::InvalidUtf8 { .. } => ScanDiagnosticKind::InvalidUtf8,
        RepositoryError::ChangedDuringRead { .. } => ScanDiagnosticKind::ChangedDuringRead,
        RepositoryError::InvalidIgnoreRule { .. } => ScanDiagnosticKind::InvalidIgnore,
        _ => ScanDiagnosticKind::Traversal,
    };
    ScanDiagnostic {
        path,
        kind,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock must follow the Unix epoch")
                .as_nanos();
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "codeatlas-repository-{label}-{}-{nonce}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn repository(path: &Path) -> RepositoryIdentity {
        RepositoryIdentity::derive(RepositoryRoot::new(path).expect("authorize test root"))
            .expect("derive repository identity")
    }

    fn write(path: impl AsRef<Path>, contents: &[u8]) {
        fs::write(path, contents).expect("write test file");
    }

    #[test]
    fn rejected_source_is_incomplete_in_full_and_targeted_scans() {
        let root = TestDir::new("invalid-not-deleted");
        let relative = RelativeSourcePath::new("a.rs").expect("relative path");
        let scanner = FileScanner::new(SourceReader::new(
            repository(root.path()),
            ScanPolicy::new(32, 100, 100, Vec::<String>::new()).expect("policy"),
        ));
        for bytes in [vec![0xff], vec![0], vec![b'a'; 33]] {
            write(root.path().join("a.rs"), &bytes);
            assert!(!scanner.scan().complete);
            assert!(
                !scanner
                    .scan_paths(
                        std::slice::from_ref(&relative),
                        &CancellationContext::default()
                    )
                    .complete
            );
        }
        fs::remove_file(root.path().join("a.rs")).expect("real deletion");
        assert!(scanner.scan().complete);
        assert!(
            scanner
                .scan_paths(&[relative], &CancellationContext::default())
                .complete
        );
    }

    #[test]
    fn relative_paths_reject_traversal_and_foreign_platform_forms() {
        for invalid in [
            "../secret",
            "/absolute",
            "C:/Windows/file",
            r"dir\file",
            "./file",
            "src//lib.rs",
            "src/lib.rs/",
            "src/./lib.rs",
            "src/../lib.rs",
        ] {
            assert!(
                RelativeSourcePath::new(invalid).is_err(),
                "accepted {invalid}"
            );
        }
        assert!(RelativeSourcePath::new("src/lib.rs").is_ok());
    }

    #[test]
    fn relative_path_validation_has_canonical_property_across_generated_inputs() {
        let atoms = ["src", "lib.rs", ".", "..", "", "éclair.rs"];
        for left in atoms {
            for middle in atoms {
                for right in atoms {
                    let input = [left, middle, right].join("/");
                    let expected = [left, middle, right]
                        .iter()
                        .all(|segment| !segment.is_empty() && !matches!(*segment, "." | ".."));
                    assert_eq!(
                        RelativeSourcePath::new(input.clone()).is_ok(),
                        expected,
                        "canonical-path property differed for {input:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn reader_rejects_prefix_tricks_secrets_and_ignored_files() {
        let parent = TestDir::new("boundaries");
        let root = parent.path().join("repo");
        let sibling = parent.path().join("repo2");
        fs::create_dir(&root).expect("create root");
        fs::create_dir(&sibling).expect("create sibling");
        fs::create_dir(root.join("hidden")).expect("create ignored directory");
        write(root.join("allowed.rs"), b"fn allowed() {}\n");
        write(root.join("ignored.rs"), b"fn ignored() {}\n");
        write(root.join(".gitignore"), b"ignored.rs\nhidden/\n");
        write(root.join("hidden/.gitignore"), b"!secret.rs\n");
        write(
            root.join("hidden/secret.rs"),
            b"fn must_stay_ignored() {}\n",
        );
        write(root.join(".env.local"), b"SYNTHETIC_SECRET=test\n");
        write(root.join("README.md"), b"untrusted instructions\n");
        write(root.join("bundle.min.js"), b"function generated(){}\n");
        write(sibling.join("outside.rs"), b"fn outside() {}\n");
        let reader = SourceReader::new(repository(&root), ScanPolicy::default());

        assert!(
            reader
                .read(&RelativeSourcePath::new("allowed.rs").expect("valid path"))
                .is_ok()
        );
        assert!(matches!(
            reader.read(&RelativeSourcePath::new("ignored.rs").expect("valid path")),
            Err(RepositoryError::Excluded { .. })
        ));
        assert!(matches!(
            reader.read(&RelativeSourcePath::new(".env.local").expect("valid path")),
            Err(RepositoryError::Excluded { .. })
        ));
        assert!(matches!(
            reader.read(&RelativeSourcePath::new("README.md").expect("valid path")),
            Err(RepositoryError::Excluded { .. })
        ));
        assert!(matches!(
            reader.read(&RelativeSourcePath::new("bundle.min.js").expect("valid path")),
            Err(RepositoryError::Excluded { .. })
        ));
        assert!(matches!(
            reader.read(&RelativeSourcePath::new("hidden/secret.rs").expect("valid path")),
            Err(RepositoryError::Excluded { .. })
        ));
        assert!(RelativeSourcePath::new("../repo2/outside.rs").is_err());
    }

    #[test]
    fn scanner_honors_all_ignore_files_and_reports_content_limits() {
        let root = TestDir::new("scan");
        fs::create_dir(root.path().join("src")).expect("create source directory");
        write(root.path().join("keep.rs"), b"fn keep() {}\n");
        write(root.path().join("gitignored.rs"), b"ignored\n");
        write(root.path().join("ignored.rs"), b"ignored\n");
        write(root.path().join("custom.rs"), b"ignored\n");
        write(root.path().join(".gitignore"), b"gitignored.rs\n");
        write(root.path().join(".ignore"), b"ignored.rs\n");
        write(root.path().join(".codeatlasignore"), b"custom.rs\n");
        write(root.path().join("binary.rs"), b"a\0b");
        write(root.path().join("invalid.rs"), &[0xff, 0xfe]);
        write(root.path().join("large.rs"), b"12345678901234567");
        let policy = ScanPolicy::new(16, 100, 100, Vec::<String>::new()).expect("valid policy");
        let result = FileScanner::new(SourceReader::new(repository(root.path()), policy)).scan();
        assert!(!result.complete);
        assert!(result.files.iter().all(|file| {
            !matches!(
                file.relative_path.as_str(),
                "gitignored.rs" | "ignored.rs" | "custom.rs"
            )
        }));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.kind == ScanDiagnosticKind::Binary)
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.kind == ScanDiagnosticKind::InvalidUtf8)
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.kind == ScanDiagnosticKind::TooLarge)
        );
    }

    #[cfg(unix)]
    #[test]
    fn scanner_marks_permission_and_resource_limit_failures_incomplete() {
        use std::os::unix::fs::PermissionsExt;

        let root = TestDir::new("scan-failures");
        let unreadable = root.path().join("unreadable");
        fs::create_dir(&unreadable).expect("create unreadable fixture directory");
        write(unreadable.join("hidden.rs"), b"fn hidden() {}\n");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))
            .expect("remove fixture permissions");
        let result = FileScanner::new(SourceReader::new(
            repository(root.path()),
            ScanPolicy::default(),
        ))
        .scan();
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o700))
            .expect("restore fixture permissions");
        assert!(!result.complete);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.kind == ScanDiagnosticKind::Traversal)
        );

        write(root.path().join("one.rs"), b"fn one() {}\n");
        write(root.path().join("two.rs"), b"fn two() {}\n");
        let limited = FileScanner::new(SourceReader::new(
            repository(root.path()),
            ScanPolicy::new(DEFAULT_MAX_FILE_BYTES, 1, 100, Vec::<String>::new())
                .expect("valid limited policy"),
        ))
        .scan();
        assert!(!limited.complete);
        assert!(
            limited
                .diagnostics
                .iter()
                .any(|item| item.kind == ScanDiagnosticKind::LimitReached)
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_final_component_replacement_are_not_followed() {
        use std::os::unix::fs::symlink;

        let root = TestDir::new("symlinks");
        let outside = root
            .path()
            .parent()
            .expect("test root has parent")
            .join(format!(
                "outside-{}",
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
        write(&outside, b"outside secret\n");
        write(root.path().join("race.rs"), b"safe\n");
        symlink(&outside, root.path().join("link.rs")).expect("create symlink");
        let reader = SourceReader::new(repository(root.path()), ScanPolicy::default());
        assert!(matches!(
            reader.read(&RelativeSourcePath::new("link.rs").expect("valid path")),
            Err(RepositoryError::LinkNotAllowed { .. })
        ));

        let race = RelativeSourcePath::new("race.rs").expect("valid path");
        let raced = reader.read_internal(&race, || {
            fs::remove_file(root.path().join("race.rs")).expect("remove race target");
            symlink(&outside, root.path().join("race.rs")).expect("replace with symlink");
        });
        assert!(raced.is_err());
        let _ = fs::remove_file(outside);
    }

    #[test]
    fn gitless_and_distinct_git_worktrees_have_distinct_ids() {
        let gitless = TestDir::new("gitless");
        let gitless_identity = repository(gitless.path());
        assert_eq!(gitless_identity.kind(), RepositoryKind::Gitless);

        let main = TestDir::new("main-worktree");
        let linked = TestDir::new("linked-worktree");
        let gix_repository = gix::init(main.path()).expect("initialize fixture repository");
        let common = gix_repository.common_dir().to_owned();
        let worktree_admin = common.join("worktrees/linked");
        fs::create_dir_all(&worktree_admin).expect("create linked worktree metadata");
        write(
            linked.path().join(".git"),
            format!("gitdir: {}\n", worktree_admin.display()).as_bytes(),
        );
        write(worktree_admin.join("commondir"), b"../..\n");
        write(worktree_admin.join("HEAD"), b"ref: refs/heads/linked\n");
        write(
            worktree_admin.join("gitdir"),
            format!("{}\n", linked.path().join(".git").display()).as_bytes(),
        );

        let main_identity = repository(main.path());
        let linked_identity = repository(linked.path());
        assert_eq!(main_identity.kind(), RepositoryKind::MainWorktree);
        assert_eq!(linked_identity.kind(), RepositoryKind::LinkedWorktree);
        assert_eq!(
            main_identity.common_git_dir(),
            linked_identity.common_git_dir()
        );
        assert_ne!(main_identity.id(), linked_identity.id());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_names_are_rejected_without_becoming_identities() {
        use std::os::unix::ffi::OsStringExt;

        let name = OsString::from_vec(vec![b'b', b'a', b'd', 0xff]);
        assert!(RelativeSourcePath::from_path(Path::new(&name)).is_err());
    }
}
