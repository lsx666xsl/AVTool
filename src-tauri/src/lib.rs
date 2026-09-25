// 解析核心按库形式组织：h264/mp4 为可独立复用的解析模块，
// commands 为 Tauri 命令薄封装。mp4::parse_mp4_bytes 等暂无 UI 入口，
// 供后续“转码/MP4 工具”路线图功能复用（有单测覆盖）。
pub mod commands;
pub mod ffmpeg;
pub mod h264;
pub mod model;
pub mod mp4;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use commands::StreamEntry;

/// 已解析文件的缓存：裸流 + 来源格式 + NALU 定位表。
/// MP4 需要先提取为 AnnexB 才能按 NALU 分析，提取与扫描结果按
/// 路径+修改时间缓存，避免每次点击/滚动都重新提取与扫描。
pub struct StreamCache(pub Mutex<HashMap<String, std::sync::Arc<StreamEntry>>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(StreamCache(Mutex::new(HashMap::new())))
        .manage(ffmpeg::FfmpegJobs(Arc::new(Mutex::new(None))))
        .invoke_handler(tauri::generate_handler![
            commands::parse_stream,
            commands::nalu_detail,
            commands::read_stream_bytes,
            commands::compare_sps,
            ffmpeg::ffmpeg_detect,
            ffmpeg::ffmpeg_probe,
            ffmpeg::ffmpeg_decode_check,
            ffmpeg::ffmpeg_decode_frame,
            ffmpeg::ffmpeg_remux,
            ffmpeg::ffmpeg_transcode,
            ffmpeg::ffmpeg_cancel
        ])
        .run(tauri::generate_context!())
        .expect("AVTool 启动失败");
}
