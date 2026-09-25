//! ffmpeg 集成模块：环境检测、媒体探测、解码验证、单帧预览、重封装、转码。
//! 通过调用系统 ffmpeg/ffprobe 命令行实现（不经 shell，参数数组直传）。

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// GUI 程序中禁止子进程弹出控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn command(program: &str) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

fn next_job_id() -> u32 {
    static ID: AtomicU32 = AtomicU32::new(1);
    ID.fetch_add(1, Ordering::SeqCst)
}

// ============ 环境检测 ============

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegEnv {
    pub available: bool,
    pub version: String,
    pub ffprobe: bool,
}

fn detect_sync() -> FfmpegEnv {
    let ok = command("ffmpeg").arg("-version").output();
    match ok {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let version = text.lines().next().unwrap_or("ffmpeg").to_string();
            let ffprobe = command("ffprobe")
                .arg("-version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            FfmpegEnv { available: true, version, ffprobe }
        }
        _ => FfmpegEnv {
            available: false,
            version: String::new(),
            ffprobe: false,
        },
    }
}

#[tauri::command]
pub async fn ffmpeg_detect() -> FfmpegEnv {
    tauri::async_runtime::spawn_blocking(detect_sync)
        .await
        .unwrap_or(FfmpegEnv {
            available: false,
            version: "检测失败".into(),
            ffprobe: false,
        })
}

// ============ 媒体探测（ffprobe -json） ============

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaProbe {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub pix_fmt: String,
    pub fps: f64,
    pub duration_sec: f64,
    pub frame_count: Option<u64>,
    pub format_name: String,
    pub has_audio: bool,
}

#[derive(Deserialize)]
struct ProbeJson {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    #[serde(default)]
    format: ProbeFormat,
}

#[derive(Deserialize)]
struct ProbeStream {
    codec_name: Option<String>,
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    r_frame_rate: Option<String>,
    nb_frames: Option<String>,
}

#[derive(Deserialize, Default)]
struct ProbeFormat {
    duration: Option<String>,
    format_name: Option<String>,
}

fn parse_fraction(s: &str) -> Option<f64> {
    let (a, b) = s.split_once('/')?;
    let na: f64 = a.trim().parse().ok()?;
    let nb: f64 = b.trim().parse().ok()?;
    if nb <= 0.0 {
        return None;
    }
    Some(na / nb)
}

fn probe_sync(path: &str) -> Result<MediaProbe, String> {
    let out = command("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe 启动失败（请确认已安装 ffmpeg）: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("ffprobe 失败: {}", err.lines().last().unwrap_or("")));
    }
    let json: ProbeJson = serde_json::from_slice(&out.stdout).map_err(|e| format!("解析 ffprobe 输出失败: {e}"))?;

    let video = json
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"))
        .ok_or("文件中没有视频轨道")?;
    let fps = video
        .r_frame_rate
        .as_deref()
        .and_then(parse_fraction)
        .unwrap_or(0.0);
    let duration = json
        .format
        .duration
        .as_deref()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);
    Ok(MediaProbe {
        codec: video.codec_name.clone().unwrap_or_else(|| "?".into()),
        width: video.width.unwrap_or(0),
        height: video.height.unwrap_or(0),
        pix_fmt: video.pix_fmt.clone().unwrap_or_else(|| "?".into()),
        fps,
        duration_sec: duration,
        frame_count: video.nb_frames.as_deref().and_then(|s| s.parse().ok()),
        format_name: json.format.format_name.unwrap_or_else(|| "?".into()),
        has_audio: json
            .streams
            .iter()
            .any(|s| s.codec_type.as_deref() == Some("audio")),
    })
}

#[tauri::command]
pub async fn ffmpeg_probe(path: String) -> Result<MediaProbe, String> {
    tauri::async_runtime::spawn_blocking(move || probe_sync(&path))
        .await
        .map_err(|e| format!("后台任务失败: {e}"))?
}

// ============ 解码验证（码流能否完整解出） ============

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub frames: u64,
    pub error_count: usize,
    /// 截断后的错误行（最多 200 条）
    pub errors: Vec<String>,
}

fn check_sync(path: &str, app: AppHandle) -> Result<CheckReport, String> {
    let mut child = command("ffmpeg")
        .args(["-nostdin", "-v", "error", "-stats", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-f", "null", "-"])
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg 启动失败: {e}"))?;
    let stderr = child.stderr.take().ok_or("无法读取 ffmpeg 输出")?;
    let mut reader = BufReader::new(stderr);
    let mut buf = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut frames = 0u64;
    loop {
        buf.clear();
        // ffmpeg 的 -stats 进度用 \r 覆盖同一行，错误行用 \n，两种都按行读
        let n = reader.read_until(b'\n', &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&buf);
        for part in line.split('\r') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some(rest) = part.strip_prefix("frame=") {
                let v: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(f) = v.parse::<u64>() {
                    frames = frames.max(f);
                }
            } else if errors.len() < 200 {
                errors.push(part.to_string());
            }
        }
        // 每 64 帧上报一次进度
        if frames % 64 == 0 {
            let _ = app.emit("ff-progress", FfProgress {
                phase: "check".into(),
                frame: frames,
                out_time_sec: 0.0,
                speed: String::new(),
                total_size: 0,
            });
        }
    }
    let _ = child.wait();
    Ok(CheckReport {
        frames,
        error_count: errors.len(),
        errors,
    })
}

#[tauri::command]
pub async fn ffmpeg_decode_check(path: String, app: AppHandle) -> Result<CheckReport, String> {
    tauri::async_runtime::spawn_blocking(move || check_sync(&path, app))
        .await
        .map_err(|e| format!("后台任务失败: {e}"))?
}

// ============ 单帧解码（PNG 预览） ============

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

fn decode_frame_sync(path: &str, frame: u64) -> Result<String, String> {
    let select = format!("select=eq(n\\,{frame})");
    let out = command("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args(["-vf", &select, "-frames:v", "1", "-f", "image2", "-c:v", "png", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("ffmpeg 启动失败: {e}"))?;
    if out.stdout.is_empty() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("解码帧 #{frame} 失败: {}", err.lines().last().unwrap_or("无输出")));
    }
    Ok(b64(&out.stdout))
}

#[tauri::command]
pub async fn ffmpeg_decode_frame(path: String, frame: u64) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || decode_frame_sync(&path, frame))
        .await
        .map_err(|e| format!("后台任务失败: {e}"))?
}

// ============ 重封装为 MP4（供浏览器播放裸流） ============

fn remux_sync(path: &str) -> Result<String, String> {
    // 已经是 MP4/MOV 的直接返回原路径（浏览器可直接播）
    let probe = probe_sync(path)?;
    if probe.format_name.contains("mp4") || probe.format_name.contains("mov") {
        return Ok(path.to_string());
    }
    let hash = path.len().checked_mul(2654435761).unwrap_or(0) as u32 ^ next_job_id();
    let out = std::env::temp_dir().join(format!("avtool_preview_{hash:08x}.mp4"));
    let out_str = out.to_string_lossy().to_string();
    let output = command("ffmpeg")
        .args(["-nostdin", "-v", "error", "-y", "-i"])
        .arg(path)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(&out_str)
        .output()
        .map_err(|e| format!("ffmpeg 启动失败: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("重封装失败: {}", err.lines().last().unwrap_or("")));
    }
    Ok(out_str)
}

#[tauri::command]
pub async fn ffmpeg_remux(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || remux_sync(&path))
        .await
        .map_err(|e| format!("后台任务失败: {e}"))?
}

// ============ 转码（进度事件 + 可取消） ============

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfProgress {
    pub phase: String,
    pub frame: u64,
    pub out_time_sec: f64,
    pub speed: String,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscodeResult {
    pub success: bool,
    pub code: i32,
    pub error_tail: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscodeRequest {
    pub input: String,
    pub output: String,
    /// 附加编码参数（如 ["-c:v","libx264","-profile:v","baseline",...]）
    pub args: Vec<String>,
}

/// 转码子进程句柄（同一时刻最多一个任务，用于取消）
pub struct FfmpegJobs(pub Arc<Mutex<Option<Child>>>);

#[tauri::command]
pub async fn ffmpeg_transcode(
    req: TranscodeRequest,
    jobs: State<'_, FfmpegJobs>,
    app: AppHandle,
) -> Result<TranscodeResult, String> {
    let input = req.input;
    let output = req.output;
    let args = req.args;
    let jobs_arc = Arc::clone(&jobs.0);

    tauri::async_runtime::spawn_blocking(move || {
        let mut child = command("ffmpeg")
            .args(["-nostdin", "-hide_banner", "-y", "-i"])
            .arg(&input)
            .args(&args)
            .args(["-progress", "pipe:1", "-nostats"])
            .arg(&output)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("ffmpeg 启动失败: {e}"))?;
        let stdout = child.stdout.take().ok_or("无法读取进度输出")?;
        let stderr = child.stderr.take().ok_or("无法读取错误输出")?;

        // 登记到任务表供取消；登记前先把 stdio 取走，避免 take 后重复写入
        *jobs_arc.lock().unwrap() = Some(child);

        // stderr 错误收集线程（保尾部若干行）
        let err_buf = Arc::new(Mutex::new(Vec::<String>::new()));
        let err_buf2 = Arc::clone(&err_buf);
        let stderr_thread = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let mut buf = err_buf2.lock().unwrap();
                buf.push(line);
                if buf.len() > 200 {
                    buf.remove(0);
                }
            }
        });

        // stdout：ffmpeg -progress 的 key=value 块，块尾 progress=continue/end
        let mut progress = FfProgress {
            phase: "transcode".into(),
            frame: 0,
            out_time_sec: 0.0,
            speed: String::new(),
            total_size: 0,
        };
        let mut kv: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim().to_string();
            if let Some((k, v)) = line.split_once('=') {
                kv.insert(k.trim().to_string(), v.trim().to_string());
            }
            if kv.contains_key("progress") {
                progress.frame = kv.get("frame").and_then(|v| v.parse().ok()).unwrap_or(progress.frame);
                progress.out_time_sec = kv
                    .get("out_time_us")
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(|us| us as f64 / 1e6)
                    .unwrap_or(progress.out_time_sec);
                progress.speed = kv.get("speed").cloned().unwrap_or_default();
                progress.total_size = kv.get("total_size").and_then(|v| v.parse().ok()).unwrap_or(progress.total_size);
                let _ = app.emit("ff-progress", progress.clone());
                kv.clear();
            }
        }

        // stdout EOF：ffmpeg 已退出，取回句柄等待并回收
        let (success, code, cancelled) = {
            let mut guard = jobs_arc.lock().unwrap();
            match guard.take() {
                Some(mut c) => match c.wait() {
                    Ok(s) => (s.success(), s.code().unwrap_or(-1), false),
                    Err(_) => (false, -1, false),
                },
                None => (false, -1, true), // 已被取消
            }
        };
        let _ = stderr_thread.join();
        if !cancelled {
            *jobs_arc.lock().unwrap() = None;
        }

        let tail: Vec<String> = err_buf.lock().unwrap().clone();
        let start = tail.len().saturating_sub(5);
        let _ = app.emit("ff-progress", FfProgress {
            phase: if success { "done".into() } else { "failed".into() },
            ..progress
        });
        Ok(TranscodeResult {
            success,
            code,
            error_tail: if cancelled {
                "已取消".into()
            } else {
                tail[start..].join("\n")
            },
        })
    })
    .await
    .map_err(|e| format!("后台任务失败: {e}"))?
}

#[tauri::command]
pub async fn ffmpeg_cancel(jobs: State<'_, FfmpegJobs>) -> Result<bool, String> {
    let mut guard = jobs.0.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = guard.take() {
        let killed = child.kill().is_ok();
        let _ = child.wait();
        Ok(killed)
    } else {
        Ok(false)
    }
}
