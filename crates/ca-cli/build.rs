use std::{env, ffi::OsString, io, process::Command};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-env-changed=RUSTC");
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let output = Command::new(rustc).arg("--version").output()?;
    if !output.status.success() {
        return Err(io::Error::other("rustc --version failed"));
    }
    let version = String::from_utf8_lossy(&output.stdout);
    println!("cargo:rustc-env=CODEATLAS_BUILD_RUSTC={}", version.trim());
    println!(
        "cargo:rustc-env=CODEATLAS_BUILD_TARGET={}",
        env::var("TARGET").unwrap_or_else(|_| "unknown-target".to_owned())
    );
    Ok(())
}
