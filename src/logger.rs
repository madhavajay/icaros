use std::fs::OpenOptions;
use std::io::Write;
use chrono::Utc;

pub fn log_to_file(message: &str) {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let log_message = format!("[{}] {}\n", timestamp, message);
    
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("logs/unified.log")
    {
        let _ = file.write_all(log_message.as_bytes());
        let _ = file.flush();
    }
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        crate::logger::log_to_file(&format!($($arg)*));
    };
}