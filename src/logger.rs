pub use log;
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::io::Write;

struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, r: &Record<'_>) {
        let c = match r.level() {
            Level::Error => 'e',
            Level::Warn => 'w',
            Level::Info => 'i',
            Level::Debug => 'd',
            Level::Trace => 't',
        };
        let mut msg = r.args().to_string();

        // Safety: \n bytes are guaranteed to be \n characters in Utf-8, and can safely be
        // replaced with \0 characters.
        for byte in unsafe { msg.as_bytes_mut() } {
            if *byte == b'\n' {
                *byte = b'\0';
            }
        }

        let mut err = std::io::stderr().lock();
        writeln!(err, "{c}: {msg}").ok();
        err.flush().ok();
    }

    fn flush(&self) {}
}

pub fn init_logger(max: LevelFilter) {
    static LOGGER: StderrLogger = StderrLogger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(max);
    }

    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
}
