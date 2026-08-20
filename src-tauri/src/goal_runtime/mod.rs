pub mod commands;
pub mod compiler;
pub mod engine;
pub mod models;
pub mod repository;
pub mod verifier;

pub fn init_goal_runtime_tables(conn: &rusqlite::Connection) -> Result<(), String> {
    repository::init_goal_runtime_tables(conn)
}
