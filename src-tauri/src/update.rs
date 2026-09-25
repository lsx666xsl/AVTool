//! 应用信息与更新检查：构建时嵌入 git 提交号，运行时查询 GitHub main 分支
//! 最新提交进行比对，发现新提交即提示"有更新"（拉取代码重新构建即可升级）。
//! 浏览器演示模式的更新通知由前端 mock 层提供。

use serde::{Deserialize, Serialize};

const REPO_API: &str = "https://api.github.com/repos/lsx666xsl/AVTool/commits/main";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    /// 构建时的 git 短提交号（无 git 环境构建时为空，此时不启用更新检查）
    pub git_sha: String,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_sha: option_env!("AVTOOL_GIT_SHA").unwrap_or("").to_string(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub latest_sha: String,
    pub message: String,
    pub date: String,
}

#[derive(Deserialize)]
struct GhCommit {
    sha: String,
    commit: GhCommitInner,
}

#[derive(Deserialize)]
struct GhCommitInner {
    message: String,
    #[serde(default)]
    committer: GhPerson,
}

#[derive(Deserialize, Default)]
struct GhPerson {
    date: Option<String>,
}

fn check_sync(local_sha: &str) -> Result<UpdateInfo, String> {
    // Windows 10+ 自带 curl.exe，无需引入 HTTP 依赖
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("curl");
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        c
    };
    #[cfg(not(windows))]
    let mut cmd = std::process::Command::new("curl");
    let out = cmd
        .args([
            "-s",
            "--max-time",
            "10",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: AVTool-UpdateCheck",
            REPO_API,
        ])
        .output()
        .map_err(|e| format!("无法发起更新检查: {e}"))?;
    if !out.status.success() {
        return Err(format!("更新检查失败（HTTP {}）", out.status));
    }
    let commit: GhCommit =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("解析 GitHub 响应失败: {e}"))?;
    let available = !local_sha.is_empty() && !commit.sha.starts_with(local_sha);
    let message = commit
        .commit
        .message
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    let date = commit
        .commit
        .committer
        .date
        .unwrap_or_default()
        .get(..10)
        .unwrap_or("")
        .to_string();
    Ok(UpdateInfo {
        available,
        latest_sha: commit.sha.chars().take(8).collect(),
        message,
        date,
    })
}

#[tauri::command]
pub async fn update_check() -> Result<UpdateInfo, String> {
    let local_sha = option_env!("AVTOOL_GIT_SHA").unwrap_or("").to_string();
    tauri::async_runtime::spawn_blocking(move || check_sync(&local_sha))
        .await
        .map_err(|e| format!("后台任务失败: {e}"))?
}
