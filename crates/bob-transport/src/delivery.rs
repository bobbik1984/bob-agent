//! Blocking native transport phase, deliberately without a database reference.
//! Claim in LocalApp, release its lock, call this, then complete in LocalApp.
//! Host should run this phase on a blocking worker, not the UI thread.
use crate::{control::HomeRequest, Limits, PinnedClient, ServerName, TransportError};
use bob_kernel::dispatch::{DispatchJob, JobKind, TransportResult, WireIntent};
use std::net::SocketAddr;

pub struct HomeDelivery<'a> {
    pub client: &'a PinnedClient,
    pub address: SocketAddr,
    pub server_name: ServerName<'static>,
    pub limits: Limits,
}
impl HomeDelivery<'_> {
    pub fn run_delivery(&self, job: &DispatchJob) -> TransportResult {
        let request = match (&job.intent, job.kind) {
            (WireIntent::Message(r), JobKind::Send) => HomeRequest::Append(r.clone()),
            (WireIntent::Message(r), JobKind::Lookup) => HomeRequest::Lookup(r.clone()),
            (WireIntent::Cancel(r), JobKind::Send) => HomeRequest::Cancel(r.clone()),
            (WireIntent::Cancel(r), JobKind::Lookup) => HomeRequest::LookupCancel(r.clone()),
        };
        match self.client.control(
            self.address,
            self.server_name.clone(),
            &request,
            self.limits,
        ) {
            Ok(result) => match result.result.receipt {
                Some(receipt) => TransportResult::Receipt(receipt),
                None if job.kind == JobKind::Lookup => TransportResult::Absent,
                None => TransportResult::Uncertain,
            },
            Err(failure) => {
                // Once a send might have reached Home, even a bad reply cannot
                // justify another append. Preserve ambiguity for exact lookup.
                if job.kind == JobKind::Send && failure.may_have_been_sent {
                    return TransportResult::Uncertain;
                }
                match failure.error {
                    TransportError::Configuration
                    | TransportError::PeerMismatch
                    | TransportError::Authentication
                    | TransportError::InvalidLimits
                    | TransportError::FrameLimit
                    | TransportError::Protocol => TransportResult::InvalidConfiguration,
                    TransportError::Io(ref e) if e.kind() == std::io::ErrorKind::InvalidData => {
                        TransportResult::InvalidConfiguration
                    }
                    TransportError::Io(_) if job.kind == JobKind::Lookup => {
                        TransportResult::Unavailable
                    }
                    TransportError::Io(_) => TransportResult::ConfirmedZero,
                }
            }
        }
    }
}
