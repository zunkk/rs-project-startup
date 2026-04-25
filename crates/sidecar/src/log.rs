use std::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Local;
use once_cell::sync::Lazy;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_panic::panic_hook;
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{filter, prelude::*};

use crate::prelude::*;

/// Defines local tracing macros that automatically attach a fixed `module` field.
///
/// The generated macros are exported from the current module with `pub(crate)`
/// visibility, so sibling modules can import them with `use super::log::info`.
#[macro_export]
macro_rules! define_module_log_macros {
    ($module:expr) => {
        $crate::define_module_log_macros!(@inner $module, $);
    };
    (@inner $module:expr, $d:tt) => {
        macro_rules! __module_trace {
            (target: $d target:expr, $d($d arg:tt)+) => {
                $crate::tracing::trace!(target: $d target, module = $module, $d($d arg)+)
            };
            ($d($d arg:tt)+) => {
                $crate::tracing::trace!(module = $module, $d($d arg)+)
            };
        }

        macro_rules! __module_debug {
            (target: $d target:expr, $d($d arg:tt)+) => {
                $crate::tracing::debug!(target: $d target, module = $module, $d($d arg)+)
            };
            ($d($d arg:tt)+) => {
                $crate::tracing::debug!(module = $module, $d($d arg)+)
            };
        }

        macro_rules! __module_info {
            (target: $d target:expr, $d($d arg:tt)+) => {
                $crate::tracing::info!(target: $d target, module = $module, $d($d arg)+)
            };
            ($d($d arg:tt)+) => {
                $crate::tracing::info!(module = $module, $d($d arg)+)
            };
        }

        macro_rules! __module_warn {
            (target: $d target:expr, $d($d arg:tt)+) => {
                $crate::tracing::warn!(target: $d target, module = $module, $d($d arg)+)
            };
            ($d($d arg:tt)+) => {
                $crate::tracing::warn!(module = $module, $d($d arg)+)
            };
        }

        macro_rules! __module_error {
            (target: $d target:expr, $d($d arg:tt)+) => {
                $crate::tracing::error!(target: $d target, module = $module, $d($d arg)+)
            };
            ($d($d arg:tt)+) => {
                $crate::tracing::error!(module = $module, $d($d arg)+)
            };
        }

        #[allow(unused_imports)]
        pub(crate) use __module_debug as debug;
        #[allow(unused_imports)]
        pub(crate) use __module_error as error;
        #[allow(unused_imports)]
        pub(crate) use __module_info as info;
        #[allow(unused_imports)]
        pub(crate) use __module_trace as trace;
        #[allow(unused_imports)]
        pub(crate) use __module_warn as warn;
    };
}

static PREPARE_STATE: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
const CURRENT_LOG_FILE_NAME: &str = "app.log";

fn prepare_log_file(log_dir: &Path, archive_file_name: &str) -> Result<()> {
    let current_log_file_path = log_dir.join(CURRENT_LOG_FILE_NAME);
    let Ok(metadata) = current_log_file_path.symlink_metadata() else {
        return Ok(());
    };

    if metadata.is_file() {
        archive_log_file(log_dir, &current_log_file_path, archive_file_name)?;
    } else if metadata.file_type().is_symlink() {
        let target_path = fs::read_link(&current_log_file_path)
            .wrap_err("failed to read current log file symlink")?;
        let target_path = if target_path.is_absolute() {
            target_path
        } else {
            log_dir.join(target_path)
        };

        if target_path.is_file() {
            archive_log_file(log_dir, &target_path, archive_file_name)?;
        }
        fs::remove_file(&current_log_file_path).wrap_err("failed to remove current log symlink")?;
    }

    Ok(())
}

fn archive_log_file(log_dir: &Path, log_file_path: &Path, archive_file_name: &str) -> Result<()> {
    fs::rename(
        log_file_path,
        unique_archive_path(log_dir, archive_file_name),
    )
    .wrap_err("failed to rename log file")
}

fn unique_archive_path(log_dir: &Path, archive_file_name: &str) -> PathBuf {
    let archive_path = log_dir.join(archive_file_name);
    if !archive_path.exists() {
        return archive_path;
    }

    let Some(file_stem) = Path::new(archive_file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
    else {
        return archive_path;
    };
    let extension = Path::new(archive_file_name)
        .extension()
        .and_then(|extension| extension.to_str());

    for index in 1.. {
        let file_name = match extension {
            Some(extension) => format!("{file_stem}-{index}.{extension}"),
            None => format!("{file_stem}-{index}"),
        };
        let path = log_dir.join(file_name);
        if !path.exists() {
            return path;
        }
    }

    archive_path
}

fn build_file_appender(
    log_dir: impl AsRef<Path>,
    max_log_files: u64,
) -> Result<RollingFileAppender> {
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_suffix("log")
        .latest_symlink(CURRENT_LOG_FILE_NAME)
        .max_log_files(max_log_files as usize)
        .build(log_dir)
        .wrap_err("failed to initialize rolling file appender")
}

#[derive(Clone, Copy)]
struct ColorLogFormatter;

struct LogField {
    name: String,
    value: String,
}

struct LogEventParts {
    level: Level,
    timestamp: String,
    message: Option<String>,
    module: Option<String>,
    fields: Vec<LogField>,
}

#[derive(Default)]
struct LogFieldVisitor {
    message: Option<String>,
    module: Option<String>,
    fields: Vec<LogField>,
}

impl LogFieldVisitor {
    fn record_field(&mut self, field: &Field, value: impl Into<String>) {
        let name = field.name();
        let value = clean_log_value(&value.into());
        match name {
            "message" => self.message = Some(value),
            "module" => self.module = Some(value),
            _ => self.fields.push(LogField {
                name: name.to_string(),
                value,
            }),
        }
    }
}

impl Visit for LogFieldVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record_field(field, value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_field(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_field(field, value.to_string());
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.record_field(field, value.to_string());
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.record_field(field, value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_field(field, value.to_string());
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_field(field, value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_field(field, format!("{value:?}"));
    }
}

fn event_parts_from_event(event: &Event<'_>) -> LogEventParts {
    let mut visitor = LogFieldVisitor::default();
    event.record(&mut visitor);
    visitor
        .fields
        .sort_by(|left, right| left.name.cmp(&right.name));

    LogEventParts {
        level: *event.metadata().level(),
        timestamp: Local::now().format("%m/%d %H:%M:%S%.3f").to_string(),
        message: visitor.message,
        module: visitor.module,
        fields: visitor.fields,
    }
}

fn clean_log_value(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_string()
}

fn format_log_line(event: &LogEventParts, ansi: bool) -> std::result::Result<String, fmt::Error> {
    let mut line = String::new();

    write_colored(
        &mut line,
        ansi,
        level_style(event.level),
        event.level.as_str(),
    )?;
    write!(line, " ")?;
    write_colored(&mut line, ansi, "\x1b[2m", &event.timestamp)?;

    if let Some(message) = event
        .message
        .as_deref()
        .filter(|message| !message.is_empty())
    {
        write!(line, " ")?;
        write_colored(&mut line, ansi, "\x1b[1m", message)?;
    }

    if let Some(module) = event.module.as_deref().filter(|module| !module.is_empty()) {
        write!(line, " ")?;
        write_colored(&mut line, ansi, "\x1b[35m", "[")?;
        write_colored(&mut line, ansi, "\x1b[1;35m", module)?;
        write_colored(&mut line, ansi, "\x1b[35m", "]")?;
    }

    if !event.fields.is_empty() {
        write!(line, " ")?;
    }
    for (index, field) in event.fields.iter().enumerate() {
        if index > 0 {
            write!(line, " ")?;
        }
        write_colored(&mut line, ansi, "\x1b[34m", &field.name)?;
        write!(line, "=")?;
        line.write_str(&field.value)?;
    }

    writeln!(line)?;
    Ok(line)
}

fn write_colored(
    writer: &mut impl fmt::Write,
    ansi: bool,
    color: &str,
    value: &str,
) -> fmt::Result {
    if ansi {
        write!(writer, "{color}{value}\x1b[0m")
    } else {
        writer.write_str(value)
    }
}

fn level_style(level: Level) -> &'static str {
    match level {
        Level::ERROR => "\x1b[1;31m",
        Level::WARN => "\x1b[1;33m",
        Level::INFO => "\x1b[1;36m",
        Level::DEBUG | Level::TRACE => "\x1b[2m",
    }
}

impl<S, N> FormatEvent<S, N> for ColorLogFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let line = format_log_line(&event_parts_from_event(event), writer.has_ansi_escapes())?;
        writer.write_str(&line)
    }
}

pub fn default_setup() -> Option<WorkerGuard> {
    setup(Level::DEBUG, None, 14, None)
}

pub fn setup(
    log_level: Level,
    log_dir: Option<PathBuf>,
    max_log_files: u64,
    custom_filter: Option<filter::Targets>,
) -> Option<WorkerGuard> {
    let mut init_flag = PREPARE_STATE.lock().expect("logger state poisoned");
    if *init_flag {
        return None;
    }
    *init_flag = true;
    drop(init_flag);

    let mut layers = Vec::new();
    let filter = if let Some(filter) = custom_filter {
        filter
    } else {
        filter::Targets::new().with_default(log_level)
    };

    // log output to file
    let mut guard = None;
    if let Some(log_dir) = log_dir
        && !cfg!(test)
    {
        let log_dir_str = log_dir.display().to_string();
        fs::create_dir_all(&log_dir).expect("failed to create log dir: {log_dir_str}");

        let now = Local::now();
        let now_time = now.format("%Y-%m-%d %H:%M:%S%.3f %:z").to_string();
        prepare_log_file(&log_dir, &format!("{now_time}.log")).unwrap();

        let (non_blocking_appender, _guard) = tracing_appender::non_blocking(
            build_file_appender(log_dir_str, max_log_files).unwrap(),
        );
        guard = Some(_guard);
        layers.push(
            tracing_subscriber::fmt::layer()
                .event_format(ColorLogFormatter)
                .with_ansi(true)
                .with_writer(non_blocking_appender)
                .with_filter(filter.clone())
                .boxed(),
        );
    }

    // log output to console
    layers.push(
        tracing_subscriber::fmt::layer()
            .event_format(ColorLogFormatter)
            .with_ansi(true)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted_log_file_names(log_dir: &Path) -> Vec<String> {
        let mut file_names = fs::read_dir(log_dir)
            .expect("failed to read log dir")
            .map(|entry| {
                entry
                    .expect("failed to read log dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        file_names.sort();
        file_names
    }

    #[test]
    fn prepare_log_file_archives_existing_app_log() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let current_log_file = temp_dir.path().join(CURRENT_LOG_FILE_NAME);
        fs::write(&current_log_file, "current log").expect("failed to write current log file");

        prepare_log_file(temp_dir.path(), "2026-04-25 16:41:49.401 +08:00.log")
            .expect("failed to prepare log file");

        assert!(!current_log_file.exists());
        assert_eq!(
            fs::read_to_string(temp_dir.path().join("2026-04-25 16:41:49.401 +08:00.log"))
                .expect("failed to read archived log file"),
            "current log"
        );
    }

    #[cfg(unix)]
    #[test]
    fn prepare_log_file_archives_existing_app_log_symlink_target() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let current_log_file = temp_dir.path().join(CURRENT_LOG_FILE_NAME);
        let current_log_target = temp_dir.path().join("2026-04-25.log");
        fs::write(&current_log_target, "current log").expect("failed to write current log file");
        std::os::unix::fs::symlink(&current_log_target, &current_log_file)
            .expect("failed to create current log symlink");

        prepare_log_file(temp_dir.path(), "2026-04-25 16:41:49.401 +08:00.log")
            .expect("failed to prepare log file");

        assert!(!current_log_file.exists());
        assert!(!current_log_target.exists());
        assert_eq!(
            fs::read_to_string(temp_dir.path().join("2026-04-25 16:41:49.401 +08:00.log"))
                .expect("failed to read archived log file"),
            "current log"
        );
    }

    #[test]
    fn build_file_appender_keeps_app_log_as_latest_symlink() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let mut appender = build_file_appender(temp_dir.path(), 14)
            .expect("failed to build rolling file appender");
        use std::io::Write as _;
        appender
            .write_all(b"current log\n")
            .expect("failed to write log");
        appender.flush().expect("failed to flush log");

        let current_log_file = temp_dir.path().join(CURRENT_LOG_FILE_NAME);
        assert!(current_log_file.is_symlink());
        assert_eq!(
            fs::read_to_string(current_log_file).expect("failed to read current log symlink"),
            "current log\n"
        );
        assert!(
            sorted_log_file_names(temp_dir.path())
                .iter()
                .any(|file_name| file_name != CURRENT_LOG_FILE_NAME && file_name.ends_with(".log"))
        );
    }

    #[test]
    fn log_line_promotes_module_and_skips_message_field() {
        let event = LogEventParts {
            level: Level::INFO,
            timestamp: "04/25 17:03:36.976".to_string(),
            message: Some("Component started".to_string()),
            module: Some("http-server".to_string()),
            fields: vec![LogField {
                name: "elapsed".to_string(),
                value: "618.625us".to_string(),
            }],
        };

        let line = format_log_line(&event, false).expect("failed to format log line");

        assert_eq!(
            line,
            "INFO 04/25 17:03:36.976 Component started [http-server] elapsed=618.625us\n"
        );
        assert!(!line.contains("module="));
        assert!(!line.contains("message="));
    }

    #[test]
    fn log_line_omits_module_segment_when_module_is_absent() {
        let event = LogEventParts {
            level: Level::INFO,
            timestamp: "04/25 17:03:36.976".to_string(),
            message: Some("App is running".to_string()),
            module: None,
            fields: Vec::new(),
        };

        let line = format_log_line(&event, false).expect("failed to format log line");

        assert_eq!(line, "INFO 04/25 17:03:36.976 App is running\n");
        assert!(!line.contains("    "));
    }

    #[test]
    fn log_line_colors_output_when_ansi_is_enabled() {
        let event = LogEventParts {
            level: Level::WARN,
            timestamp: "04/25 17:03:36.976".to_string(),
            message: Some("Slow request".to_string()),
            module: Some("http".to_string()),
            fields: vec![LogField {
                name: "time_cost".to_string(),
                value: "1.2s".to_string(),
            }],
        };

        let colored_line = format_log_line(&event, true).expect("failed to format log line");
        let plain_line = format_log_line(&event, false).expect("failed to format log line");

        assert!(colored_line.contains("\x1b["));
        assert!(!plain_line.contains("\x1b["));
        assert!(colored_line.ends_with('\n'));
    }
}
