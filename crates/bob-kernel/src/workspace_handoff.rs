//! Multi-workspace state inventory (`BP4-01`) and explicit verifiable
//! cross-node handoff (`BP4-02`, `BP4-03`). Never infers workspace equivalence
//! from directory names alone; blocks with a single-sentence reason on
//! unpushed offline source, dirty-tree mismatch, missing assets/LFS, diverged
//! branch, or interrupted sync.
use crate::{
    app::LocalApp,
    outbox::{digest, Error, Result},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInventoryItem {
    pub workspace_id: String,
    pub project_id: Option<String>,
    pub device_id: String,
    pub path_ref: String,
    pub availability_state: String,
    pub availability_reason: String,
    pub head_commit: String,
    pub branch: String,
    pub upstream_delta_ahead: i64,
    pub upstream_delta_behind: i64,
    pub consistency_state: String,
    pub work_state: String,
    pub dirty_manifest_sha256: String,
    pub missing_assets: Vec<String>,
    pub lease_state: Option<String>,
    pub fencing_token: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceHandoff {
    pub id: String,
    pub project_id: String,
    pub conversation_id: Option<String>,
    pub source_workspace_id: String,
    pub target_workspace_id: String,
    pub checkpoint_kind: String,
    pub base_commit: String,
    pub patch_bundle_sha256: Option<String>,
    pub dirty_manifest_sha256: String,
    pub required_assets: Vec<String>,
    pub state: String,
    pub blocked_reason: Option<String>,
    pub target_fencing_token: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceStateUpdate {
    pub workspace_id: String,
    pub project_id: Option<String>,
    pub device_id: String,
    pub path_ref: String,
    pub availability_state: String,
    pub availability_reason: String,
    pub head_commit: String,
    pub branch: String,
    pub upstream_delta_ahead: i64,
    pub upstream_delta_behind: i64,
    pub consistency_state: String,
    pub work_state: String,
    pub dirty_manifest_sha256: String,
    pub missing_assets: Vec<String>,
}

impl LocalApp {
    pub fn record_workspace_state(
        &mut self,
        update: &WorkspaceStateUpdate,
        now: i64,
    ) -> Result<WorkspaceInventoryItem> {
        if update.workspace_id.trim().is_empty()
            || update.device_id.trim().is_empty()
            || update.path_ref.trim().is_empty()
            || !matches!(
                update.availability_state.as_str(),
                "online" | "sleeping" | "offline"
            )
            || !matches!(
                update.consistency_state.as_str(),
                "synced" | "ahead" | "behind" | "diverged" | "unverified"
            )
            || !matches!(
                update.work_state.as_str(),
                "clean"
                    | "dirty_uncommitted"
                    | "stashed"
                    | "patch_bundle_pending"
                    | "sync_failed"
            )
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO devices(id,label,trust_state,created_at) VALUES(?1,?1,'unpaired',?2)",
            params![update.device_id.trim(), now],
        )?;
        tx.execute(
            "INSERT INTO workspaces(id,device_id,directory_ref,policy_ref,version) VALUES(?1,?2,?3,'replica-workspace-v1',1)
             ON CONFLICT(id) DO UPDATE SET device_id=excluded.device_id,directory_ref=excluded.directory_ref,version=workspaces.version+1",
            params![
                update.workspace_id.trim(),
                update.device_id.trim(),
                update.path_ref.trim()
            ],
        )?;
        if let Some(pid) = update.project_id.as_deref().filter(|s| !s.trim().is_empty()) {
            tx.execute(
                "INSERT OR IGNORE INTO projects(id,title,acceptance,status,version,created_at,updated_at) VALUES(?1,?1,'local','active',1,?2,?2)",
                params![pid.trim(), now],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO project_workspaces(project_id,workspace_id) VALUES(?1,?2)",
                params![pid.trim(), update.workspace_id.trim()],
            )?;
        }
        let dirty_json = json!({
            "dirty_manifest_sha256": update.dirty_manifest_sha256.trim()
        })
        .to_string();
        let missing_json = serde_json::to_string(&update.missing_assets)?;
        tx.execute(
            "INSERT INTO workspace_states(workspace_id,availability_state,availability_reason,head_commit,branch,upstream_delta_ahead,upstream_delta_behind,consistency_state,work_state,dirty_summary_json,missing_assets_json,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(workspace_id) DO UPDATE SET
               availability_state=excluded.availability_state,
               availability_reason=excluded.availability_reason,
               head_commit=excluded.head_commit,
               branch=excluded.branch,
               upstream_delta_ahead=excluded.upstream_delta_ahead,
               upstream_delta_behind=excluded.upstream_delta_behind,
               consistency_state=excluded.consistency_state,
               work_state=excluded.work_state,
               dirty_summary_json=excluded.dirty_summary_json,
               missing_assets_json=excluded.missing_assets_json,
               updated_at=excluded.updated_at",
            params![
                update.workspace_id.trim(),
                update.availability_state.as_str(),
                update.availability_reason.trim(),
                update.head_commit.trim(),
                update.branch.trim(),
                update.upstream_delta_ahead,
                update.upstream_delta_behind,
                update.consistency_state.as_str(),
                update.work_state.as_str(),
                dirty_json,
                missing_json,
                now
            ],
        )?;
        let item = query_workspace_inventory_item(&tx, update.workspace_id.trim())?;
        tx.commit()?;
        Ok(item)
    }

    pub fn workspace_inventory(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<WorkspaceInventoryItem>> {
        let mut stmt = self.store.db.prepare(
            "SELECT w.id
             FROM workspaces w
             JOIN workspace_states ws ON ws.workspace_id = w.id
             LEFT JOIN project_workspaces pw ON pw.workspace_id = w.id
             WHERE (?1 IS NULL OR pw.project_id = ?1)
             ORDER BY ws.updated_at DESC, w.id ASC",
        )?;
        let ids: Vec<String> = stmt
            .query_map([project_id], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(query_workspace_inventory_item(&self.store.db, &id)?);
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn initiate_workspace_handoff(
        &mut self,
        handoff_id: &str,
        project_id: &str,
        conversation_id: Option<&str>,
        source_workspace_id: &str,
        target_workspace_id: &str,
        checkpoint_kind: &str,
        base_commit: &str,
        patch_bundle_sha256: Option<&str>,
        dirty_manifest_sha256: &str,
        required_assets: &[String],
        now: i64,
    ) -> Result<WorkspaceHandoff> {
        if [
            handoff_id,
            project_id,
            source_workspace_id,
            target_workspace_id,
            base_commit,
            dirty_manifest_sha256,
        ]
        .iter()
        .any(|s| s.trim().is_empty())
            || source_workspace_id == target_workspace_id
            || !matches!(
                checkpoint_kind,
                "git_commit_pushed" | "patch_bundle_pushed"
            )
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active_lease: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace_leases WHERE workspace_id=?1 AND state IN ('held','stop_pending','unknown'))",
            [source_workspace_id],
            |r| r.get(0),
        )?;
        if active_lease {
            return Err(Error::InvalidTransition);
        }
        let source = query_workspace_inventory_item(&tx, source_workspace_id)?;
        let _target = query_workspace_inventory_item(&tx, target_workspace_id)?;

        let (initial_state, blocked_reason) = if source.availability_state == "offline"
            && (source.upstream_delta_ahead > 0
                || source.work_state == "dirty_uncommitted"
                || source.consistency_state == "ahead")
        {
            (
                "blocked",
                Some(
                    "源节点在未推送本地提交或未打包脏工作树前已离线，无法直接切换续写；请唤醒源节点推送或显式从远端已同步基线新建分支。"
                        .to_owned(),
                ),
            )
        } else if checkpoint_kind == "patch_bundle_pushed"
            && patch_bundle_sha256.map(str::trim).unwrap_or("").is_empty()
        {
            (
                "blocked",
                Some("当前检查点要求补丁包摘要，但未提供已推送的补丁包校验和。".to_owned()),
            )
        } else {
            ("checkpoint_created", None)
        };

        let assets_json = serde_json::to_string(required_assets)?;
        tx.execute(
            "INSERT INTO workspace_handoffs(id,project_id,conversation_id,source_workspace_id,target_workspace_id,checkpoint_kind,base_commit,patch_bundle_sha256,dirty_manifest_sha256,required_assets_json,state,blocked_reason,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)",
            params![
                handoff_id.trim(),
                project_id.trim(),
                conversation_id.map(str::trim).filter(|s| !s.is_empty()),
                source_workspace_id.trim(),
                target_workspace_id.trim(),
                checkpoint_kind,
                base_commit.trim(),
                patch_bundle_sha256.map(str::trim).filter(|s| !s.is_empty()),
                dirty_manifest_sha256.trim(),
                assets_json,
                initial_state,
                blocked_reason,
                now
            ],
        )?;
        let handoff = query_handoff(&tx, handoff_id.trim())?;
        tx.commit()?;
        Ok(handoff)
    }

    /// Verify target workspace replica consistency, dirty tree manifest, and
    /// required assets before atomically transferring the project workspace
    /// fencing token to the target workspace.
    pub fn verify_and_complete_workspace_handoff(
        &mut self,
        handoff_id: &str,
        now: i64,
    ) -> Result<WorkspaceHandoff> {
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let handoff = query_handoff(&tx, handoff_id)?;
        if handoff.state == "completed" {
            tx.commit()?;
            return Ok(handoff);
        }
        let source = query_workspace_inventory_item(&tx, &handoff.source_workspace_id)?;
        let target = query_workspace_inventory_item(&tx, &handoff.target_workspace_id)?;

        let mut block_reason: Option<String> = None;

        // Fault 1: Source node shut down / went offline before pushing commits or dirty tree
        if source.availability_state == "offline"
            && (source.upstream_delta_ahead > 0
                || source.work_state == "dirty_uncommitted"
                || source.consistency_state == "ahead")
        {
            block_reason = Some(
                "源节点在未推送提交或未同步脏工作树前已关机离线，禁止在目标节点基于旧代码静默续写。"
                    .to_owned(),
            );
        }
        // Fault 5: Target workspace sync interrupted or unverified
        else if target.work_state == "sync_failed"
            || target.consistency_state == "unverified"
        {
            block_reason = Some(
                "目标节点工作区同步中断或状态未验证，请先完成同步校验后再接力执行。"
                    .to_owned(),
            );
        }
        // Fault 4: Target workspace diverged from upstream / checkpoint base commit
        else if target.consistency_state == "diverged"
            || (target.upstream_delta_ahead > 0 && target.upstream_delta_behind > 0)
            || target.head_commit != handoff.base_commit
        {
            block_reason = Some(format!(
                "目标节点工作区提交 ({}) 与交接基线 ({}) 分叉或不一致，已阻断自动续写。",
                target.head_commit, handoff.base_commit
            ));
        }
        // Fault 2: Same commit, different dirty tree
        else if target.dirty_manifest_sha256 != handoff.dirty_manifest_sha256 {
            block_reason = Some(
                "目标节点虽处于相同 Git Commit，但本地脏工作树摘要与交接检查点不一致，已阻断隐式续写。"
                    .to_owned(),
            );
        }
        // Fault 3: Missing local assets or Git LFS objects on target
        else {
            let missing: Vec<String> = handoff
                .required_assets
                .iter()
                .filter(|req| target.missing_assets.iter().any(|m| m == *req))
                .cloned()
                .collect();
            if !missing.is_empty() || !target.missing_assets.is_empty() {
                let list = if !missing.is_empty() {
                    missing.join(", ")
                } else {
                    target.missing_assets.join(", ")
                };
                block_reason = Some(format!(
                    "目标节点缺失必要的本地资产或 LFS 对象 ({list})，已阻断跨节点接力。"
                ));
            } else if target.availability_state != "online" {
                block_reason = Some(
                    "目标执行节点当前不在线，无法立即接管工作区写租约。".to_owned(),
                );
            }
        }

        if let Some(reason) = block_reason {
            tx.execute(
                "UPDATE workspace_handoffs SET state='blocked',blocked_reason=?2,updated_at=?3 WHERE id=?1",
                params![handoff_id, reason, now],
            )?;
            let updated = query_handoff(&tx, handoff_id)?;
            tx.commit()?;
            return Ok(updated);
        }

        let epoch: i64 = tx.query_row(
            "SELECT epoch FROM home_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let prev_source_token: i64 = tx
            .query_row(
                "SELECT fencing_token FROM workspace_leases WHERE workspace_id=?1",
                [&handoff.source_workspace_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let prev_target_token: i64 = tx
            .query_row(
                "SELECT fencing_token FROM workspace_leases WHERE workspace_id=?1",
                [&handoff.target_workspace_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let next_token = prev_source_token.max(prev_target_token) + 1;

        let handoff_exec_id = format!("handoff-fence:{}", digest(handoff_id.as_bytes()));
        let handoff_task_id = format!("handoff-task:{}", digest(handoff_id.as_bytes()));
        tx.execute(
            "INSERT OR IGNORE INTO tasks(id,project_id,source_message_id,title,status,constraints_json,version,created_at,updated_at)
             VALUES(?1,?2,NULL,'workspace handoff fence','completed','{}',1,?3,?3)",
            params![handoff_task_id, handoff.project_id, now],
        )?;
        let contract = json!({
            "kind": "workspace_handoff_fence",
            "handoff_id": handoff_id,
            "base_commit": handoff.base_commit,
            "dirty_manifest_sha256": handoff.dirty_manifest_sha256,
            "fencing_token": next_token
        })
        .to_string();
        tx.execute(
            "INSERT OR IGNORE INTO executions(id,project_id,task_id,workspace_id,request_id,status,requested_json,contract_json,contract_sha256,version,ended_at)
             VALUES(?1,?2,?3,?4,?5,'completed','{}',?6,?7,1,?8)",
            params![
                handoff_exec_id,
                handoff.project_id,
                handoff_task_id,
                handoff.target_workspace_id,
                format!("handoff-req:{handoff_id}"),
                contract,
                digest(contract.as_bytes()),
                now
            ],
        )?;
        tx.execute(
            "UPDATE workspace_leases SET state='released',updated_at=?2 WHERE workspace_id=?1",
            params![handoff.source_workspace_id, now],
        )?;
        tx.execute(
            "INSERT INTO workspace_leases(workspace_id,execution_id,home_epoch,fencing_token,state,updated_at)
             VALUES(?1,?2,?3,?4,'released',?5)
             ON CONFLICT(workspace_id) DO UPDATE SET
               execution_id=excluded.execution_id,
               home_epoch=excluded.home_epoch,
               fencing_token=excluded.fencing_token,
               state='released',
               updated_at=excluded.updated_at",
            params![
                handoff.target_workspace_id,
                handoff_exec_id,
                epoch,
                next_token,
                now
            ],
        )?;
        tx.execute(
            "UPDATE workspace_handoffs SET state='completed',blocked_reason=NULL,updated_at=?2 WHERE id=?1",
            params![handoff_id, now],
        )?;
        let completed = query_handoff(&tx, handoff_id)?;
        tx.commit()?;
        Ok(completed)
    }
}

fn query_workspace_inventory_item(
    db: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<WorkspaceInventoryItem> {
    let row: Option<(
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
        String,
        String,
        String,
        String,
        Option<String>,
        i64,
        i64,
    )> = db
        .query_row(
            "SELECT w.id,
                    (SELECT pw.project_id FROM project_workspaces pw WHERE pw.workspace_id=w.id LIMIT 1),
                    w.device_id,
                    w.directory_ref,
                    ws.availability_state,
                    ws.availability_reason,
                    ws.head_commit,
                    ws.branch,
                    ws.upstream_delta_ahead,
                    ws.upstream_delta_behind,
                    ws.consistency_state,
                    ws.work_state,
                    ws.dirty_summary_json,
                    ws.missing_assets_json,
                    wl.state,
                    COALESCE(wl.fencing_token, 0),
                    ws.updated_at
             FROM workspaces w
             JOIN workspace_states ws ON ws.workspace_id = w.id
             LEFT JOIN workspace_leases wl ON wl.workspace_id = w.id
             WHERE w.id = ?1",
            [workspace_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                    r.get(12)?,
                    r.get(13)?,
                    r.get(14)?,
                    r.get(15)?,
                    r.get(16)?,
                ))
            },
        )
        .optional()?;
    let (
        ws_id,
        project_id,
        device_id,
        path_ref,
        availability_state,
        availability_reason,
        head_commit,
        branch,
        upstream_delta_ahead,
        upstream_delta_behind,
        consistency_state,
        work_state,
        dirty_json,
        missing_json,
        lease_state,
        fencing_token,
        updated_at,
    ) = row.ok_or(Error::InvalidTransition)?;

    let dirty_val: serde_json::Value =
        serde_json::from_str(&dirty_json).unwrap_or_else(|_| json!({}));
    let dirty_manifest_sha256 = dirty_val
        .get("dirty_manifest_sha256")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let missing_assets: Vec<String> = serde_json::from_str(&missing_json).unwrap_or_default();

    Ok(WorkspaceInventoryItem {
        workspace_id: ws_id,
        project_id,
        device_id,
        path_ref,
        availability_state,
        availability_reason,
        head_commit,
        branch,
        upstream_delta_ahead,
        upstream_delta_behind,
        consistency_state,
        work_state,
        dirty_manifest_sha256,
        missing_assets,
        lease_state,
        fencing_token,
        updated_at,
    })
}

fn query_handoff(db: &rusqlite::Connection, handoff_id: &str) -> Result<WorkspaceHandoff> {
    let row: Option<(
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
        i64,
        i64,
    )> = db
        .query_row(
            "SELECT h.id, h.project_id, h.conversation_id, h.source_workspace_id, h.target_workspace_id,
                    h.checkpoint_kind, h.base_commit, h.patch_bundle_sha256, h.dirty_manifest_sha256,
                    h.required_assets_json, h.state, h.blocked_reason,
                    (SELECT wl.fencing_token FROM workspace_leases wl WHERE wl.workspace_id = h.target_workspace_id),
                    h.created_at, h.updated_at
             FROM workspace_handoffs h
             WHERE h.id = ?1",
            [handoff_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                    r.get(12)?,
                    r.get(13)?,
                    r.get(14)?,
                ))
            },
        )
        .optional()?;
    let (
        id,
        project_id,
        conversation_id,
        source_workspace_id,
        target_workspace_id,
        checkpoint_kind,
        base_commit,
        patch_bundle_sha256,
        dirty_manifest_sha256,
        assets_json,
        state,
        blocked_reason,
        target_fencing_token,
        created_at,
        updated_at,
    ) = row.ok_or(Error::InvalidTransition)?;
    let required_assets: Vec<String> = serde_json::from_str(&assets_json).unwrap_or_default();
    Ok(WorkspaceHandoff {
        id,
        project_id,
        conversation_id,
        source_workspace_id,
        target_workspace_id,
        checkpoint_kind,
        base_commit,
        patch_bundle_sha256,
        dirty_manifest_sha256,
        required_assets,
        state,
        blocked_reason,
        target_fencing_token,
        created_at,
        updated_at,
    })
}
