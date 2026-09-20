use std::{
    error::Error,
    fs,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct Layout {
    base: std::path::PathBuf,
    root: std::path::PathBuf,
    home: std::path::PathBuf,
}

impl Layout {
    fn new(label: &str) -> Result<Self, Box<dyn Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "codeatlas-cli-{label}-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        let root = base.join("repo");
        let home = base.join("home");
        fs::create_dir_all(&root)?;
        fs::create_dir_all(&home)?;
        Ok(Self { base, root, home })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codeatlas"));
        command.env("HOME", &self.home);
        command
    }

    fn json(&self, arguments: &[&str]) -> Result<Value, Box<dyn Error>> {
        let output = self.command().args(arguments).output()?;
        assert_success(&output);
        Ok(serde_json::from_slice(&output.stdout)?)
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn codeatlas() -> Command {
    Command::new(env!("CARGO_BIN_EXE_codeatlas"))
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn help_lists_only_implemented_commands() -> Result<(), Box<dyn Error>> {
    let output = codeatlas().arg("--help").output()?;
    assert_success(&output);
    let stdout = String::from_utf8(output.stdout)?;
    for implemented in ["serve", "doctor", "index", "status"] {
        assert!(stdout.contains(implemented));
    }
    for unimplemented in ["search", "memory"] {
        assert!(!stdout.contains(unimplemented));
    }
    let serve = codeatlas().args(["serve", "--help"]).output()?;
    assert_success(&serve);
    let serve = String::from_utf8(serve.stdout)?;
    assert!(serve.contains("--watch"));
    assert!(serve.contains("--memory-write"));
    Ok(())
}

#[test]
fn version_is_available_without_starting_the_server() -> Result<(), Box<dyn Error>> {
    let output = codeatlas().arg("--version").output()?;
    assert_success(&output);
    assert_eq!(String::from_utf8(output.stdout)?.trim(), "codeatlas 0.1.0");
    Ok(())
}

#[test]
fn doctor_reports_real_storage_and_parser_capabilities() -> Result<(), Box<dyn Error>> {
    let layout = Layout::new("doctor")?;
    let output = layout
        .command()
        .args(["doctor", "--json", "--root"])
        .arg(&layout.root)
        .output()?;
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["schema_version"], ca_storage::CURRENT_SCHEMA_VERSION);
    assert!(
        report["configuration"]["repository_id"]
            .as_str()
            .ok_or("missing repository ID")?
            .starts_with("wt-")
    );
    assert_eq!(report["capabilities"]["database"], "ready");
    assert_eq!(report["capabilities"]["storage_role"], "owner");
    assert_eq!(report["capabilities"]["fts5"], true);
    assert_eq!(report["capabilities"]["parsers"], "available_on_index");
    assert_eq!(report["capabilities"]["startup_network"], false);
    assert_eq!(
        report["capabilities"]["tools"],
        serde_json::json!([
            "repository_status",
            "index_repository",
            "job_status",
            "cancel_job",
            "search_symbols",
            "get_symbol",
            "find_references",
            "trace_calls",
            "get_file_outline",
            "read_code",
            "get_repo_map",
            "analyze_impact",
            "build_context",
            "search_memories"
        ])
    );
    assert_eq!(report["capabilities"]["memory_writes"], false);
    assert_eq!(
        report["capabilities"]["resources"].as_array().map(Vec::len),
        Some(4)
    );
    assert_eq!(
        report["capabilities"]["prompts"].as_array().map(Vec::len),
        Some(3)
    );
    assert!(output.stderr.is_empty());
    Ok(())
}

#[test]
fn index_persists_every_supported_language_and_reuses_after_restart() -> Result<(), Box<dyn Error>>
{
    let layout = Layout::new("languages")?;
    let seeds = [
        ("seed.dart", "class DartSeed {}\n"),
        ("Seed.cs", "class CSharpSeed {}\n"),
        ("seed.rs", "pub fn rust_seed() {}\n"),
        ("seed.go", "package seed\nfunc GoSeed() {}\n"),
        ("Seed.java", "class JavaSeed {}\n"),
        ("seed.js", "function jsSeed() {}\n"),
        ("seed.jsx", "function JsxSeed(){ return <div/>; }\n"),
        ("seed.ts", "function tsSeed(): void {}\n"),
        ("seed.tsx", "function TsxSeed(){ return <div/>; }\n"),
    ];
    for (path, source) in seeds {
        fs::write(layout.root.join(path), source)?;
    }

    let root = layout.root.to_str().ok_or("non-UTF-8 root")?;
    let first = layout.json(&["index", "--root", root, "--json"])?;
    assert_eq!(first["state"], "completed");
    assert_eq!(first["files_discovered"], 9);
    assert_eq!(first["files_parsed"], 9);
    assert_eq!(first["files_persisted"], 9);
    assert_eq!(first["files_failed"], 0);

    let restarted = layout.json(&["status", "--root", root, "--json"])?;
    assert_eq!(restarted["active_files"], 9);
    assert!(restarted["active_facts"].as_u64().unwrap_or(0) >= 9);
    assert!(restarted["active_symbols"].as_u64().unwrap_or(0) >= 9);

    let no_op = layout.json(&["index", "--root", root, "--json"])?;
    assert_eq!(no_op["files_reused"], 9);
    assert_eq!(no_op["files_parsed"], 0);
    assert_eq!(no_op["files_persisted"], 9);
    let full = layout.json(&["index", "--root", root, "--full", "--json"])?;
    assert_eq!(full["files_reused"], 0);
    assert_eq!(full["files_parsed"], 9);
    assert_eq!(full["files_persisted"], 9);
    Ok(())
}

#[test]
fn changed_content_deletion_and_rename_have_exact_progress() -> Result<(), Box<dyn Error>> {
    let layout = Layout::new("incremental")?;
    for (path, source) in [
        ("a.rs", "pub fn alpha() {}\n"),
        ("b.rs", "pub fn bravo() {}\n"),
        ("c.rs", "pub fn charlie() {}\n"),
        ("d.rs", "pub fn delta() {}\n"),
    ] {
        fs::write(layout.root.join(path), source)?;
    }
    let root = layout.root.to_str().ok_or("non-UTF-8 root")?;
    let first = layout.json(&["index", "--root", root, "--json"])?;
    let old_generation = first["generation_id"].as_str().ok_or("generation")?;

    let changed = layout.root.join("a.rs");
    let old_modified = fs::metadata(&changed)?.modified()?;
    fs::write(&changed, "pub fn alpine() {}\n")?;
    std::fs::File::options()
        .write(true)
        .open(&changed)?
        .set_times(std::fs::FileTimes::new().set_modified(old_modified))?;
    fs::remove_file(layout.root.join("b.rs"))?;
    fs::rename(layout.root.join("c.rs"), layout.root.join("renamed.rs"))?;

    let second = layout.json(&["index", "--root", root, "--json"])?;
    assert_ne!(second["generation_id"], old_generation);
    assert_eq!(second["files_discovered"], 3);
    assert_eq!(second["files_reused"], 1);
    assert_eq!(second["files_parsed"], 2);
    assert_eq!(second["files_deleted"], 2);
    assert_eq!(second["files_persisted"], 3);
    let status = layout.json(&["status", "--root", root, "--json"])?;
    assert_eq!(status["active_files"], 3);
    assert_eq!(status["active_facts"], 3);
    Ok(())
}

#[test]
fn request_key_is_durable_and_failed_scan_preserves_active_generation() -> Result<(), Box<dyn Error>>
{
    let layout = Layout::new("dedup-failure")?;
    fs::write(layout.root.join("seed.rs"), "pub fn seed() {}\n")?;
    let root = layout.root.to_str().ok_or("non-UTF-8 root")?;
    let first = layout.json(&[
        "index",
        "--root",
        root,
        "--request-key",
        "same-request",
        "--json",
    ])?;
    let generation = first["generation_id"]
        .as_str()
        .ok_or("generation")?
        .to_owned();
    let repeated = layout.json(&[
        "index",
        "--root",
        root,
        "--request-key",
        "same-request",
        "--json",
    ])?;
    assert_eq!(repeated["deduplicated"], true);
    assert_eq!(repeated["job_id"], first["job_id"]);
    assert_eq!(repeated["generation_id"], generation);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let unreadable = layout.root.join("unreadable");
        fs::create_dir(&unreadable)?;
        fs::write(unreadable.join("hidden.rs"), "pub fn hidden() {}\n")?;
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))?;
        let failed = layout
            .command()
            .args(["index", "--root", root, "--json"])
            .output()?;
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o700))?;
        assert!(!failed.status.success());
        assert!(String::from_utf8(failed.stderr)?.contains("scan was incomplete"));
        let status = layout.json(&["status", "--root", root, "--json"])?;
        assert_eq!(status["active_generation_id"], generation);
        assert_eq!(status["latest_job"]["state"], "failed");
    }
    Ok(())
}

#[test]
fn relative_roots_are_rejected_before_work_begins() -> Result<(), Box<dyn Error>> {
    for command in ["serve", "index", "status"] {
        let output = codeatlas()
            .args([command, "--root", "relative-root"])
            .output()?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8(output.stderr)?.contains("must be absolute"));
    }
    Ok(())
}
