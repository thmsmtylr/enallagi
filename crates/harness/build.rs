use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=adapters/presets");
    println!("cargo:rerun-if-changed=runners");
    // a crate tarball is no repository, so `cargo install` from one builds as `unknown`
    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=ENALLAGI_COMMIT={commit}");
    // a commit moves the branch ref and a checkout moves HEAD; either rebuilds with the new commit
    let mut names = vec!["HEAD".to_string()];
    names.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for name in names {
        if let Some(path) = git(&["rev-parse", "--path-format=absolute", "--git-path", &name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}
