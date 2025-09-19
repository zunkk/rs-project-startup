use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Local;
use once_cell::sync::Lazy;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_panic::panic_hook;
use tracing_subscriber::fmt::format;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::{filter, prelude::*};

use crate::prelude::*;

static PREPARE_STATE: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

#[derive(Clone, Copy)]
struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, writer: &mut Writer<'_>) -> std::fmt::Result {
        let now = Local::now();
        write!(writer, "{}", now.format("%Y-%m-%d %H:%M:%S%.3f"))
    }
}

pub fn default_setup() -> Option<WorkerGuard> {
    setup(Level::DEBUG, None, 14)
}

pub fn setup(
    log_level: Level,
    log_dir: Option<PathBuf>,
    max_log_files: u64,
) -> Option<WorkerGuard> {
    let mut init_flag = PREPARE_STATE.lock().expect("Logger state poisoned");
    if *init_flag {
        return None;
    }
    *init_flag = true;
    drop(init_flag);

    let mut layers = Vec::new();

    let filter = filter::Targets::new().with_default(log_level);

    let local_time = LocalTimer;

    // log output to file
    let mut guard = None;
    if let Some(log_dir) = log_dir {
        if !cfg!(test) {
            let log_dir_str = log_dir.display().to_string();
            fs::create_dir_all(log_dir).expect("Failed to create log dir: {log_dir_str}");

            let now = Local::now();
            let today_log_file_name = now.format("%Y-%m-%d").to_string();
            let now_time = now.format("%Y-%m-%d %H:%M:%S%.3f %:z").to_string();
            let today_log_file_path = format!("{log_dir_str}/{today_log_file_name}.log");
            if fs::metadata(&today_log_file_path).is_ok() {
                fs::rename(
                    &today_log_file_path,
                    format!("{log_dir_str}/{now_time}.log"),
                )
                .wrap_err("Failed to rename log file")
                .unwrap();
            }

            let (non_blocking_appender, _guard) = tracing_appender::non_blocking(
                RollingFileAppender::builder()
                    .rotation(Rotation::DAILY)
                    .filename_suffix("log")
                    .max_log_files(max_log_files as usize)
                    .build(log_dir_str)
                    .expect("Initializing rolling file appender failed"),
            );
            guard = Some(_guard);
            layers.push(
                tracing_subscriber::fmt::layer()
                    .with_ansi(true)
                    .fmt_fields(format::Pretty::default())
                    .with_timer(local_time.clone())
                    .with_target(false)
                    .with_writer(non_blocking_appender)
                    .with_filter(filter.clone())
                    .boxed(),
            );
        }
    }

    // log output to console
    layers.push(
        tracing_subscriber::fmt::layer()
            .with_ansi(true)
            .fmt_fields(format::Pretty::default())
            .with_timer(local_time.clone())
            .with_target(false)
            .with_filter(filter.clone())
            .boxed(),
    );
    tracing_subscriber::registry().with(layers).init();

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        panic_hook(panic_info);
        prev_hook(panic_info);
    }));

    guard
}
