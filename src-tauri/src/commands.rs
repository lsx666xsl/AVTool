//! Tauri 命令层：薄封装，业务逻辑在 h264/mp4 模块。
//! 所有命令均为 async：CPU/IO 重活通过 spawn_blocking 交给独立线程，
//! 不阻塞主事件循环（避免 UI 卡顿）。

use std::sync::Arc;

use tauri::State;

use crate::h264;
use crate::h264::nal::RawNal;
use crate::model::{HexData, NaluDetail, ParseResult, SpsCompare};
use crate::mp4;
use crate::StreamCache;

const MAX_HEX_WINDOW: usize = 64 * 1024;
const MAX_FILE_SIZE: usize = 2 * 1024 * 1024 * 1024;

/// 缓存条目：裸流内容 + 来源格式 + NALU 定位表（一次扫描，处处复用）。
pub struct StreamEntry {
    pub data: Arc<Vec<u8>>,
    pub source: &'static str,
    pub nals: Arc<Vec<RawNal>>,
}

fn read_file(path: &str) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("无法访问文件 {path}: {e}"))?;
    let size = meta.len() as usize;
    if size > MAX_FILE_SIZE {
        return Err(format!("文件过大（{} MB），上限 2048 MB", size / 1024 / 1024));
    }
    std::fs::read(path).map_err(|e| format!("读取文件失败: {e}"))
}

/// 缓存键 = 路径 + 修改时间 + 大小：文件被重新保存后自动失效
fn cache_key(path: &str) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("无法访问文件 {path}: {e}"))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(format!("{path}|{}|{}", mtime, meta.len()))
}

/// 读取文件并统一为可分析的裸流（重活，应在 spawn_blocking 中执行）。
fn load_entry(path: &str) -> Result<StreamEntry, String> {
    let raw = read_file(path)?;
    if h264::nal::is_mp4(&raw) {
        let (stream, _) = mp4::extract_annexb_bytes(&raw)?;
        build_entry(stream, "mp4")
    } else if h264::nal::is_annexb(&raw) {
        build_entry(raw, "annexb")
    } else {
        Err("无法识别的文件格式：既不是 MP4 容器，也不含 H.264 AnnexB 起始码".into())
    }
}

fn build_entry(stream: Vec<u8>, source: &'static str) -> Result<StreamEntry, String> {
    let data = Arc::new(stream);
    let nals = Arc::new(h264::nal::scan_annexb(&data));
    Ok(StreamEntry { data, source, nals })
}

fn cache_get(cache: &StreamCache, path: &str) -> Option<Arc<StreamEntry>> {
    let key = cache_key(path).ok()?;
    cache.0.lock().ok()?.get(&key).cloned()
}

fn cache_put(cache: &StreamCache, path: &str, entry: &Arc<StreamEntry>) {
    if let Ok(key) = cache_key(path) {
        if let Ok(mut map) = cache.0.lock() {
            if map.len() > 8 {
                map.clear(); // 简单的容量控制
            }
            map.insert(key, Arc::clone(entry));
        }
    }
}

/// 获取（并缓存）文件的解析条目。缓存命中为 O(1)；未命中时把
/// 读文件/MP4 提取/NALU 扫描放入 spawn_blocking，不阻塞 UI。
async fn load_stream(cache: &State<'_, StreamCache>, path: &str) -> Result<Arc<StreamEntry>, String> {
    if let Some(hit) = cache_get(cache, path) {
        return Ok(hit);
    }
    let p = path.to_string();
    let entry = tauri::async_runtime::spawn_blocking(move || load_entry(&p))
        .await
        .map_err(|e| format!("后台解析任务失败: {e}"))??;
    let entry = Arc::new(entry);
    cache_put(cache, path, &entry);
    Ok(entry)
}

#[tauri::command]
pub async fn parse_stream(
    path: String,
    cache: State<'_, StreamCache>,
) -> Result<ParseResult, String> {
    let entry = load_stream(&cache, &path).await?;
    Ok(h264::parse_stream_bytes(&entry.data, entry.source, &entry.nals))
}

#[tauri::command]
pub async fn nalu_detail(
    path: String,
    index: usize,
    cache: State<'_, StreamCache>,
) -> Result<NaluDetail, String> {
    let entry = load_stream(&cache, &path).await?;
    h264::nalu_detail_bytes(&entry.data, &entry.nals, index)
}

/// 读取缓存裸流的指定区间（十六进制视图用），单次上限 64KB
#[tauri::command]
pub async fn read_stream_bytes(
    path: String,
    offset: usize,
    len: usize,
    cache: State<'_, StreamCache>,
) -> Result<HexData, String> {
    let entry = load_stream(&cache, &path).await?;
    let len = len.min(MAX_HEX_WINDOW);
    let start = offset.min(entry.data.len());
    let end = (start + len).min(entry.data.len());
    Ok(HexData {
        offset: start,
        bytes: entry.data[start..end].to_vec(),
        file_size: entry.data.len(),
    })
}

#[tauri::command]
pub async fn compare_sps(
    path_a: String,
    path_b: String,
    cache: State<'_, StreamCache>,
) -> Result<SpsCompare, String> {
    // 顺序加载：慢路径都在 spawn_blocking 线程上执行，不阻塞 UI
    let entry_a = load_stream(&cache, &path_a).await?;
    let entry_b = load_stream(&cache, &path_b).await?;
    h264::compare_sps_bytes(&entry_a.data, &entry_a.nals, &entry_b.data, &entry_b.nals)
}
