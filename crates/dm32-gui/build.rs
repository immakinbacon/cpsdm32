//! Record which commit the binary was built from, shown in Help → About and the status bar.

use std::process::Command;

fn main() {
    let sha = std::env::var("CI_COMMIT_SHORT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let out = Command::new("git")
                .args(["rev-parse", "--short=8", "HEAD"])
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        });
    println!(
        "cargo:rustc-env=DM32_BUILD={}",
        sha.unwrap_or_else(|| "unknown".into())
    );
    println!("cargo:rerun-if-env-changed=CI_COMMIT_SHORT_SHA");
    // Rebuild the stamp when the checked-out commit changes.
    for f in ["../../.git/HEAD", "../../.git/refs/heads"] {
        println!("cargo:rerun-if-changed={f}");
    }
}
