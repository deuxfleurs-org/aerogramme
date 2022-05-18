use std::time::{SystemTime, UNIX_EPOCH};

/// Returns milliseconds since UNIX Epoch
pub fn now_msec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Fix your clock :o")
        .as_millis() as u64
}
