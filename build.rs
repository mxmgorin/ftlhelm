//! Embeds the git revision of the build as FTLHELM_REV.

use std::process::Command;

fn main() {
    let rev = Command::new("git")
        .args(["describe", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=FTLHELM_REV={rev}");
    // The sources too: an unstaged edit changes what --dirty reports.
    for path in [".git/HEAD", ".git/index", ".git/refs", "src"] {
        println!("cargo:rerun-if-changed={path}");
    }
}
