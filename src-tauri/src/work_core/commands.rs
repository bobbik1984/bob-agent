use tauri::State;

use crate::db::DbState;

use super::models::{
    CreateProjectInput, CreateRelationInput, CreateWorkObjectInput, DeleteWorkObjectInput,
    ProjectAggregate, UpdateWorkStatusInput, WorkObject, WorkProject, WorkRelation,
};
use super::project_links::{
    self, DismissProjectLinkInput, ExternalLink, ProjectLinkCandidate, ProjectLinkOutcome,
    ResolveProjectLinkInput,
};
use super::{repository, snapshot};

fn refresh_snapshot(project_id: &str, aggregate: Result<ProjectAggregate, String>) {
    let outcome = aggregate.and_then(|aggregate| snapshot::write_project_snapshot(&aggregate));
    if let Err(error) = outcome {
        log::warn!("Work Core snapshot refresh failed for {project_id}: {error}");
    }
}

#[tauri::command]
pub fn work_project_list(db: State<'_, DbState>) -> Result<Vec<WorkProject>, String> {
    let conn = db.0.lock().map_err(|error| error.to_string())?;
    repository::list_projects(&conn)
}

#[tauri::command]
pub fn work_project_create(
    db: State<'_, DbState>,
    input: CreateProjectInput,
) -> Result<WorkProject, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    let project = repository::create_project(&mut conn, input)?;
    let aggregate = repository::get_project_aggregate(&conn, &project.id);
    drop(conn);
    refresh_snapshot(&project.id, aggregate);
    Ok(project)
}

#[tauri::command]
pub fn work_project_get(
    db: State<'_, DbState>,
    project_id: String,
) -> Result<ProjectAggregate, String> {
    let conn = db.0.lock().map_err(|error| error.to_string())?;
    repository::get_project_aggregate(&conn, &project_id)
}

#[tauri::command]
pub fn work_object_create(
    db: State<'_, DbState>,
    input: CreateWorkObjectInput,
) -> Result<WorkObject, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    let object = repository::create_object(&mut conn, input)?;
    let aggregate = repository::get_project_aggregate(&conn, &object.project_id);
    drop(conn);
    refresh_snapshot(&object.project_id, aggregate);
    Ok(object)
}

#[tauri::command]
pub fn work_object_update_status(
    db: State<'_, DbState>,
    input: UpdateWorkStatusInput,
) -> Result<WorkObject, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    let object = repository::update_object_status(&mut conn, input)?;
    let aggregate = repository::get_project_aggregate(&conn, &object.project_id);
    drop(conn);
    refresh_snapshot(&object.project_id, aggregate);
    Ok(object)
}

#[tauri::command]
pub fn work_object_delete(
    db: State<'_, DbState>,
    input: DeleteWorkObjectInput,
) -> Result<WorkObject, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    let object = repository::delete_object(&mut conn, input)?;
    let aggregate = repository::get_project_aggregate(&conn, &object.project_id);
    drop(conn);
    refresh_snapshot(&object.project_id, aggregate);
    Ok(object)
}

#[tauri::command]
pub fn work_relation_create(
    db: State<'_, DbState>,
    input: CreateRelationInput,
) -> Result<WorkRelation, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    let relation = repository::create_relation(&mut conn, input)?;
    let aggregate = repository::get_project_aggregate(&conn, &relation.project_id);
    drop(conn);
    refresh_snapshot(&relation.project_id, aggregate);
    Ok(relation)
}

#[tauri::command]
pub fn work_project_export_snapshot(
    db: State<'_, DbState>,
    project_id: String,
) -> Result<snapshot::SnapshotOutcome, String> {
    let conn = db.0.lock().map_err(|error| error.to_string())?;
    let aggregate = repository::get_project_aggregate(&conn, &project_id)?;
    drop(conn);
    snapshot::write_project_snapshot(&aggregate)
}

#[tauri::command]
pub fn work_project_link_list_pending(
    db: State<'_, DbState>,
    limit: Option<usize>,
) -> Result<Vec<ProjectLinkCandidate>, String> {
    let conn = db.0.lock().map_err(|error| error.to_string())?;
    project_links::list_pending(&conn, limit.unwrap_or(20))
}

#[tauri::command]
pub fn work_project_link_resolve(
    db: State<'_, DbState>,
    input: ResolveProjectLinkInput,
) -> Result<ProjectLinkOutcome, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    let outcome = project_links::resolve_candidate(&mut conn, input)?;
    let project_id = outcome.candidate.selected_project_id.clone();
    if let Some(project_id) = project_id.as_deref() {
        project_links::refresh_project_snapshot(&conn, project_id);
    }
    Ok(outcome)
}

#[tauri::command]
pub fn work_project_link_dismiss(
    db: State<'_, DbState>,
    input: DismissProjectLinkInput,
) -> Result<ProjectLinkCandidate, String> {
    let mut conn = db.0.lock().map_err(|error| error.to_string())?;
    project_links::dismiss_candidate(&mut conn, input)
}

#[tauri::command]
pub fn work_external_link_list(
    db: State<'_, DbState>,
    project_id: String,
) -> Result<Vec<ExternalLink>, String> {
    let conn = db.0.lock().map_err(|error| error.to_string())?;
    project_links::list_external_links(&conn, &project_id)
}
