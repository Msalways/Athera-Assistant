fn main() {
    tauri_build::build();

    // Embed build fingerprint for developer diagnostics.
    if let Ok(git_hash) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        let hash = String::from_utf8_lossy(&git_hash.stdout).trim().to_string();
        println!("cargo:rustc-env=GIT_HASH={}", hash);
    }
    if let Ok(git_dirty) = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
    {
        let dirty = !git_dirty.stdout.is_empty();
        println!("cargo:rustc-env=GIT_DIRTY={}", dirty);
    }
    println!(
        "cargo:rustc-env=BUILD_TIMESTAMP={}",
        chrono_free_timestamp()
    );
    println!(
        "cargo:rustc-env=CARGO_PKG_VERSION={}",
        env!("CARGO_PKG_VERSION")
    );
}

fn chrono_free_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
