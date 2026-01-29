use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::models::HardwareEvent;

pub type EventBuffer = Arc<Mutex<VecDeque<HardwareEvent>>>;

pub fn create_event_buffer() -> EventBuffer {
    Arc::new(Mutex::new(VecDeque::with_capacity(500)))
}

pub fn push_event(buffer: &EventBuffer, event: HardwareEvent) {
    let mut buf = buffer.lock().unwrap();
    if buf.len() >= 500 {
        buf.pop_front();
    }
    buf.push_back(event);
}

#[tauri::command]
pub fn get_recent_events(count: usize, state: tauri::State<'_, EventBuffer>) -> Vec<HardwareEvent> {
    let buf = state.lock().unwrap();
    buf.iter()
        .rev()
        .take(count)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
