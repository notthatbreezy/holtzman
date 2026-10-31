use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn main() {
    // Installers that build outside a checkout (cargo install --git) pass the commit
    // explicitly; a changed value always reruns this script, even in a reused target dir.
    println!("cargo:rerun-if-env-changed=HOLTZMAN_GIT_COMMIT");
    let commit = std::env::var("HOLTZMAN_GIT_COMMIT")
        .ok()
        .map(|value| value.trim().chars().take(12).collect::<String>())
        .filter(|value| !value.is_empty())
        .or_else(|| git(&["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=HOLTZMAN_GIT_COMMIT={commit}");
    // HEAD only names the branch; committing moves the branch ref, so watch that too.
    // --git-path resolves worktrees and the shared refs directory.
    let mut watched = vec!["HEAD".to_string(), "packed-refs".to_string()];
    watched.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for name in watched {
        if let Some(path) = git(&["rev-parse", "--git-path", &name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}
