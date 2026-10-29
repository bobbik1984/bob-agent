//! Bob's single local domain kernel. No transport, provider or credential access.
pub mod outbox;
pub mod prewarm;

pub mod dispatch;
pub mod inbox;
pub mod wake;

mod deadline;

pub mod app;

pub mod harness_reply;

pub mod local_execution;

pub mod home_auth;

pub mod pairing;

pub mod home_result;

pub mod replica;

pub mod home_execution;

pub mod home_host;

pub mod home_rejection;

pub mod catalog;
pub mod online_provider;
pub mod workspace_handoff;
