//! Transport-independent, restricted project application operations.
//! Only trusted authentication code constructs ProjectAccess. No RPC forwarding.
use crate::NodeContext;
use bob_kernel::project_links::ProjectLinkAuthority;
use serde_json::{json, Value};

pub(crate) struct ProjectAccess {
    pub projects: Vec<String>,
    pub sessions: Vec<(String,String,String)>,
    pub organize: bool,
}
impl ProjectAccess {
    fn project_allowed(&self, id:&Value)->bool {
        id.as_str().is_some_and(|p|self.projects.iter().any(|v|v==p))
    }
    fn session_allowed(&self,a:&Value)->bool {
        self.sessions.iter().any(|(c,h,s)| a["connection_id"]==*c && a["harness"]==*h && a["session_id"]==*s)
    }
}
fn links(ctx: &NodeContext, g: &ProjectAccess) -> Result<Value, &'static str> {
    let all=ctx.app.project_session_links().map_err(|_| "BOT_READ_FAILED")?;
    let rows=all["links"].as_array().ok_or("BOT_READ_FAILED")?.iter()
        .filter(|r| g.session_allowed(r) && g.project_allowed(&r["project_id"]))
        .map(|r| json!({"connection_id":r["connection_id"],"harness":r["harness"],"session_id":r["session_id"],"project_id":r["project_id"],"version":r["version"],"project_title":r["project_title"]})).collect::<Vec<_>>();
    Ok(json!({"links":rows}))
}
pub(crate) fn execute(ctx: &mut NodeContext, g: &ProjectAccess, name: &str, a: &Value, now: i64) -> Result<Value, &'static str> {
    if !a.is_object() { return Err("BOT_ARGUMENTS_INVALID"); }
    let allowed: &[&str] = if name == "bob_associate_session" {
        &["connection_id","harness","session_id","project_id","expected_version"]
    } else { &[] };
    if a.as_object().unwrap().keys().any(|k| !allowed.contains(&k.as_str())) {return Err("BOT_ARGUMENTS_INVALID");}
    match name {
        "bob_projects" => {
            let cat=ctx.app.catalog_snapshot().map_err(|_| "BOT_READ_FAILED")?;
            let rows=cat.projects.iter().filter(|p| g.projects.contains(&p.id))
                .map(|p| json!({"id":p.id,"title":p.title,"status":p.status,"updated_at":p.updated_at})).collect::<Vec<_>>();
            // Deliberately no drafts, provider configuration, device credentials or message previews.
            Ok(json!({"projects":rows}))
        }
        "bob_project_sessions" => links(ctx,g),
        "bob_associate_session" => {
            let expected=a["expected_version"].as_i64().filter(|v| *v>=0 && *v<i64::MAX).ok_or("BOT_VERSION_REQUIRED")?;
            ctx.app.set_project_session_link_authorized(
                ProjectLinkAuthority::Scoped {organize:g.organize,projects:&g.projects,sessions:&g.sessions},
                a["connection_id"].as_str().ok_or("BOT_SCOPE_DENIED")?,
                a["harness"].as_str().ok_or("BOT_SCOPE_DENIED")?,
                a["session_id"].as_str().ok_or("BOT_SCOPE_DENIED")?,
                a["project_id"].as_str(),expected,now).map_err(|_| "BOT_LINK_DENIED_OR_CONFLICT")?;
            Ok(json!({"updated":true,"version":expected+1}))
        }
        _ => Err("BOT_SCOPE_DENIED"),
    }
}
