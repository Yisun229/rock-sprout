//! 日志：同时写到 `<数据目录>/logs/rock-sprout.log`（按天切分）和终端。
//!
//! 正式版没有控制台窗口，用户遇到问题时让他把日志文件发过来就行。

use std::sync::OnceLock;
use tracing_appender::non_blocking::WorkerGuard;

static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

pub fn init() {
    let dir = crate::paths::data_root().join("logs");
    let _ = std::fs::create_dir_all(&dir);

    let appender = tracing_appender::rolling::daily(&dir, "rock-sprout.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    // guard 必须活到进程结束，否则日志会被丢掉
    let _ = GUARD.set(guard);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init();

    tracing::info!(dir = %dir.display(), "日志已启动");
}
