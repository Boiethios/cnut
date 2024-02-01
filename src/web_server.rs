/// The web server allowing to expose an API to the outside world and to display
/// an user interface to monitor the network.

mod endpoints {
    mod node_status;
    pub use node_status::node_status;
    mod static_file;
    pub use static_file::static_file;
}

use crate::{
    error::{Error, Result},
    network::RunningNode,
    util::crypto::PublicKey,
};
use axum::{
    extract::State as AxumState,
    routing::{get, post},
    Router,
};
use std::{collections::HashMap, mem, path::PathBuf, sync::Arc};
use tokio::sync::{oneshot, Mutex};
use tower_http::services::ServeFile;

#[derive(Clone)]
pub struct AppState {
    running_dir: PathBuf,
    kill_senders: Arc<Mutex<Vec<(oneshot::Sender<()>, String)>>>,
    nodes: HashMap<String, AppNode>,
}

#[derive(Clone)]
struct AppNode {
    path: PathBuf,
    public_key: PublicKey,
    rpc_port: u16,
    rest_port: u16,
}

pub async fn run_server(
    running_dir: PathBuf,
    kill_senders: Vec<(oneshot::Sender<()>, String)>,
    nodes: Vec<RunningNode>,
) -> Result<()> {
    use endpoints::*;

    let state = AppState {
        running_dir,
        kill_senders: Arc::new(Mutex::new(kill_senders)),
        nodes: nodes.into_iter().map(Into::into).collect(),
    };

    let app = Router::new()
        .route_service("/", ServeFile::new("public/index.html"))
        .route_service("/index.css", ServeFile::new("public/index.css"))
        .nest(
            "/file",
            Router::new().route("/*path", get(endpoints::static_file)),
        )
        .route("/node-status", get(node_status))
        .route("/shutdown", post(shutdown))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:6532").await.unwrap();

    axum::serve(listener, app)
        .await
        .map_err(Error::StartingServerWeb)
}

async fn shutdown(AxumState(state): AxumState<AppState>) -> &'static str {
    log::debug!("Kill all nodes signal sent");
    for (sender, name) in mem::take(&mut *state.kill_senders.lock().await) {
        if let Err(_) = sender.send(()) {
            log::warn!("Kill signal could not be send to {name}, maybe it has already shut down")
        }
    }

    "Network has shut down"
}

impl From<RunningNode> for (String, AppNode) {
    fn from(value: RunningNode) -> Self {
        let node = AppNode {
            path: value.running_path,
            public_key: value.public_key,
            rpc_port: value.rpc_port,
            rest_port: value.rest_port,
        };

        (value.name, node)
    }
}
