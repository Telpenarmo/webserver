use std::{fs, time::SystemTime};

use time::{macros::format_description, OffsetDateTime};
use tracing::subscriber;
use tracing_subscriber::{
    fmt::{layer, time as fmt_time},
    layer::SubscriberExt,
    registry,
};

pub fn init() {
    let today: OffsetDateTime = SystemTime::now().into();
    let log_file_path = format!("logs/{}.log.json", today.date());
    fs::create_dir_all("logs").expect("Failed to create logs directory");
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)
        .unwrap();

    let offset = time::UtcOffset::current_local_offset().unwrap();
    let pretty_logger = layer()
        .pretty()
        .with_timer(fmt_time::OffsetTime::new(
            offset,
            format_description!("[hour]:[minute]:[second]:[subsecond digits:4]"),
        ))
        .with_file(false)
        .with_line_number(false);

    let json_logger = layer()
        .json()
        .with_writer(log_file)
        .with_thread_names(true)
        .with_file(true);

    let logger = registry().with(pretty_logger).with(json_logger);
    subscriber::set_global_default(logger).expect("Failed to set global logger.");
}
