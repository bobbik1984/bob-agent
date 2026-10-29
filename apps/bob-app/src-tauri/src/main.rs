#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if bob_app_lib::worker_entry() {
        return;
    }
    bob_app_lib::run();
}
