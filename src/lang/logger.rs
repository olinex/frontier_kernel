// @author:    olinex
// @time:      2023/09/03

// self mods

// use other mods
use log::{Level, LevelFilter, Metadata, Record, SetLoggerError};

// use self mods
use crate::configs;
use crate::println;

pub struct KernelLogger {
    level: Level,
}

impl KernelLogger {
    pub const fn new(level: Level) -> Self {
        Self { level }
    }
}

impl log::Log for KernelLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("[kernel] {} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: KernelLogger = KernelLogger::new(configs::LOG_LEVEL);

pub fn init() {
    if let Err(error) = log::set_logger(&LOGGER) {
        panic!("Could not set logger cause by {}", error);
    }
    log::set_max_level(LevelFilter::Info);
}
