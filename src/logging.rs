// SPDX-License-Identifier: GPL-3.0-or-later

use log::{error, info, warn, Record, Level, Metadata, LevelFilter};

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            rtt_target::rprintln!("{}", record.args());
        }
    }
    fn flush(&self) {}
}

static LOGGER: Logger = Logger;

pub fn init_logging() {
    rtt_target::rtt_init_print!(NoBlockSkip, 4096);
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(LevelFilter::Trace);
}
