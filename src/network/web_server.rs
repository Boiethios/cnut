mod fragments {
    mod node_status;
    pub use node_status::node_status;
}

use crate::{
    error::{Error, Result},
    util::{crypto::PublicKey, map},
};
use axum::{
    extract::State as AxumState,
    routing::{get, post},
    Router,
};
use std::{mem, sync::Arc};
use tokio::sync::{oneshot, Mutex};
use tower_http::services::ServeFile;

#[derive(Clone)]
pub struct AppState {
    kill_senders: Arc<Mutex<Vec<(oneshot::Sender<()>, String)>>>,
    nodes: Vec<AppNode>,
}

#[derive(Clone)]
struct AppNode {
    name: String,
    public_key: PublicKey,
    rpc_port: u16,
    rest_port: u16,
}

pub async fn run_server(
    kill_senders: Vec<(oneshot::Sender<()>, String)>,
    nodes: Vec<super::RunningNode>,
) -> Result<()> {
    use fragments::*;

    let state = AppState {
        kill_senders: Arc::new(Mutex::new(kill_senders)),
        nodes: nodes.into_iter().map(Into::into).collect(),
    };

    let app = Router::new()
        .route_service("/", ServeFile::new("public/index.html"))
        .route_service("/index.css", ServeFile::new("public/index.css"))
        .route("/node-status", get(node_status))
        .route("/shutdown", post(shutdown))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:6532").await.unwrap();

    axum::serve(listener, app)
        .await
        .map_err(Error::StartingServerWeb)
}

async fn shutdown(AxumState(state): AxumState<AppState>) {
    for (sender, name) in mem::take(&mut *state.kill_senders.lock().await) {
        if let Err(_) = sender.send(()) {
            log::warn!("Kill signal could not be send to {name}, maybe it has already shut down")
        }
    }
}

impl From<super::RunningNode> for AppNode {
    fn from(value: super::RunningNode) -> Self {
        Self {
            name: value.name,
            public_key: value.public_key,
            rpc_port: value.rpc_port,
            rest_port: value.rest_port,
        }
    }
}
