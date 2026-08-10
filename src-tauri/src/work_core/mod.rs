pub mod commands;
pub mod models;
pub mod project_links;
pub mod repository;
pub mod snapshot;

pub fn init_work_core_tables(conn: &rusqlite::Connection) -> Result<(), String> {
    repository::init_work_core_tables(conn)?;
    project_links::init_project_link_tables(conn)
}
