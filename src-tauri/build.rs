fn main() {
    // 构建时嵌入 git 提交号，供应用内"检查更新"与远端比对
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if !sha.is_empty() {
        println!("cargo:rustc-env=AVTOOL_GIT_SHA={sha}");
    }
    // 提交变化时触发重新编译（HEAD 变动 + 分支引用更新）
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads");
    tauri_build::build()
}
