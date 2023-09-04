// @author:    olinex
// @time:      2023/09/03

// self mods

// use other mods
use log::{LevelFilter, Metadata, Record};

// use self mods
use crate::configs;
use crate::println;

pub struct KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= configs::LOG_LEVEL
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("[kernel] {} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: KernelLogger = KernelLogger;

pub fn init() {
    if let Err(error) = log::set_logger(&LOGGER) {
        panic!("Could not set logger cause by {}", error);
    }
    log::set_max_level(LevelFilter::Info);
}
