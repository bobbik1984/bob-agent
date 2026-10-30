//! Native connection state. Only a completed pairing flow may install it;
//! renderer requests cannot supply certificates, endpoints or result payloads.
use crate::{now, with_app, AppState};
use bob_kernel::dispatch::DispatchPolicy;
use bob_transport::{
    control::HomeRequest, delivery::HomeDelivery, Limits, PinnedClient, ServerName,
};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use tauri::State;

pub(crate) struct PairedHome {
    pub client: PinnedClient,
    pub address: SocketAddr,
    pub server_name: ServerName<'static>,
}
#[derive(Default)]
pub(crate) struct HomeLinkState {
    pub connection: Mutex<Option<Arc<PairedHome>>>,
    busy: Arc<AtomicBool>,
    next_result: AtomicUsize,
}
struct Flight(Arc<AtomicBool>);
impl Drop for Flight {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub(crate) async fn app_home_sync<R: tauri::Runtime>(
    host: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    link: State<'_, HomeLinkState>,
) -> Result<&'static str, &'static str> {
    if link.busy.swap(true, Ordering::AcqRel) {
        return Ok("sync_in_progress");
    }
    let flight = Flight(link.busy.clone());
    let missing = link
        .connection
        .lock()
        .map_err(|_| "HOME_STATE_UNAVAILABLE")?
        .is_none();
    if missing {
        crate::pairing_commands::restore(&host, &state, &link).await?;
    }
    let connection = link
        .connection
        .lock()
        .map_err(|_| "HOME_STATE_UNAVAILABLE")?
        .clone();
    let Some(connection) = connection else {
        return Ok("provider_unconfigured");
    };
    let app = state.0.clone();
    let result_index = link.next_result.fetch_add(1, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        let _flight = flight;
        let policy = DispatchPolicy {
            retry_ms: 3000,
            lease_ms: 30000,
            max_attempts: 5,
        };
        let job = with_app(&app, |a| {
            a.claim_home_delivery(now()?, policy)
                .map_err(|_| "HOME_CLAIM_FAILED")
        })?;
        if let Some(job) = job {
            let transport = HomeDelivery {
                client: &connection.client,
                address: connection.address,
                server_name: connection.server_name.clone(),
                limits: Limits::default(),
            };
            let outcome = transport.run_delivery(&job);
            with_app(&app, |a| {
                a.complete_home_delivery(&job, outcome, now()?, policy)
                    .map_err(|_| "HOME_RECEIPT_UNCONFIRMED")
            })?;
        }
        let direct_turn = with_app(&app, |a| {
            a.pending_replica_direct_syncs()
                .map(|mut v| v.drain(..).next())
                .map_err(|_| "HOME_DIRECT_SYNC_READ_FAILED")
        })?;
        if let Some(turn) = direct_turn {
            if let Ok(ack) = connection.client.control(
                connection.address,
                connection.server_name.clone(),
                &HomeRequest::ReplicaDirectTurn(turn.clone()),
                Limits::default(),
            ) {
                if let Some(receipt) = ack.result.receipt {
                    let _ = with_app(&app, |a| {
                        a.mark_replica_direct_turn_synced(&turn, &receipt, now()?)
                            .map_err(|_| "HOME_DIRECT_SYNC_MARK_FAILED")
                    });
                }
            }
        }
        let personal_op = with_app(&app, |a| {
            a.pending_personal_op_syncs()
                .map(|mut v| v.drain(..).next())
                .map_err(|_| "HOME_PERSONAL_SYNC_READ_FAILED")
        })?;
        if let Some(op) = personal_op {
            if let Ok(ack) = connection.client.control(
                connection.address,
                connection.server_name.clone(),
                &HomeRequest::ReplicaPersonalOp(op.clone()),
                Limits::default(),
            ) {
                if ack.result.receipt.as_ref().is_some_and(|r| r.accepted) {
                    let _ = with_app(&app, |a| {
                        a.ack_replica_personal_op(&op.sync_id, now()?)
                            .map_err(|_| "HOME_PERSONAL_SYNC_MARK_FAILED")
                    });
                }
            }
        }
        let request = with_app(&app, |a| {
            a.pending_home_result_at(result_index)
                .map_err(|_| "HOME_RESULT_READ_FAILED")
        })?;
        // Bound a UI tick to one query as well as one delivery. No network I/O while
        // holding the SQLite/App mutex; local message saves remain responsive.
        if let Some(request) = request {
            let result = connection
                .client
                .control(
                    connection.address,
                    connection.server_name.clone(),
                    &HomeRequest::Result(request.clone()),
                    Limits::default(),
                )
                .map_err(|_| "HOME_RESULT_UNAVAILABLE")?;
            with_app(&app, |a| {
                a.apply_authenticated_home_result(&request, &result.result, now()?)
                    .map_err(|_| "HOME_RESULT_REJECTED")
            })?;
        }
        Ok("sync_checked")
    })
    .await
    .map_err(|_| "HOME_SYNC_UNCONFIRMED")?
}
