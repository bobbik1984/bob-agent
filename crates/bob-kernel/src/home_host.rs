//! Trusted native Home facade. No parallel connection or renderer-supplied identity.
use crate::{
    app::LocalApp,
    dispatch::CancelExecution,
    home_auth::HomeMessageContext,
    home_result::HomeResult,
    outbox::{AppendMessage, Error, HomeReceipt, Result},
};
impl LocalApp {
    pub fn authenticate_home_peer(
        &mut self,
        home: &str,
        epoch: i64,
        pin: &str,
        session: &str,
        now: i64,
    ) -> Result<HomeMessageContext> {
        if home != self.origin {
            return Err(Error::InvalidTransition);
        }
        self.store
            .authenticate_home_session(home, epoch, pin, session, now)
    }
    pub fn host_receive_message(
        &mut self,
        ctx: &HomeMessageContext,
        r: &AppendMessage,
        now: i64,
    ) -> Result<HomeReceipt> {
        self.store.receive_message(ctx, r, now)
    }
    pub fn host_lookup_message(
        &mut self,
        ctx: &HomeMessageContext,
        r: &AppendMessage,
        now: i64,
    ) -> Result<Option<HomeReceipt>> {
        self.store.lookup_message_receipt(ctx, r, now)
    }
    pub fn host_lookup_result(
        &mut self,
        ctx: &HomeMessageContext,
        r: &AppendMessage,
        now: i64,
    ) -> Result<HomeResult> {
        self.store.lookup_home_result(ctx, r, now)
    }
    pub fn host_receive_cancel(
        &mut self,
        ctx: &HomeMessageContext,
        r: &CancelExecution,
        now: i64,
    ) -> Result<HomeReceipt> {
        self.store.receive_cancel(ctx, r, now)
    }
    pub fn host_lookup_cancel(
        &mut self,
        ctx: &HomeMessageContext,
        r: &CancelExecution,
        now: i64,
    ) -> Result<Option<HomeReceipt>> {
        self.store.lookup_cancel_receipt(ctx, r, now)
    }
}

impl LocalApp {
    /// Durably accept a new remote message and reserve its one execution in a
    /// single SQLite transaction. Any preparation failure rolls back acceptance.
    /// Previously decided receipts are immutable and can never trigger a run.
    pub fn host_accept_and_prepare(
        &mut self,
        ctx: &HomeMessageContext,
        request: &AppendMessage,
        workspace: &str,
        grant: &crate::home_execution::ReadOnlyExecutionGrant,
        now: i64,
    ) -> Result<(HomeReceipt, Option<crate::local_execution::LocalExecution>)> {
        let tx = self
            .store
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        crate::home_auth::authorize(&tx, ctx, request, now, false)?;
        let hash = crate::outbox::digest(&serde_json::to_vec(request)?);
        let existed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM inbox_receipts WHERE request_id=?1)",
            [&request.request_id],
            |r| r.get(0),
        )?;
        let receipt = crate::inbox::receive_message_in_tx(&tx, ctx, request, &hash, now)?;
        if existed {
            tx.commit()?;
            return Ok((receipt, None));
        }
        let execution = crate::home_execution::prepare_remote_in_tx(
            &self.origin,
            &tx,
            ctx,
            request,
            workspace,
            grant,
            now,
        )?;
        tx.commit()?;
        Ok((receipt, Some(execution)))
    }
}
