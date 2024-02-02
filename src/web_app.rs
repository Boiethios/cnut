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
};
use axum::{
    extract::State as AxumState,
    routing::{get, post},
    Router,
};
use std::{path::PathBuf, sync::Arc};
use tokio::spawn;
use tower_http::services::ServeFile;

#[derive(Debug, Clone)]
struct AppState {
    nodes: Vec<RunningNode>,
    base_dir: PathBuf,
}

pub async fn serve(nodes: Vec<RunningNode>, base_dir: PathBuf) -> Result<()> {
    use endpoints::*;

    let state = AppState { nodes, base_dir };

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

    let task = async move {
        axum::serve(listener, app)
            .await
            .map_err(Error::StartingServerWeb)
    };

    spawn(task);

    println!("Web app at http://127.0.0.1:6532");
    Ok(())
}

async fn shutdown(AxumState(mut state): AxumState<AppState>) -> &'static str {
    log::debug!("Kill all nodes signal sent");
    for node in &mut state.nodes {
        if let Some(sender) = node.kill_sender.lock().await.take() {
            if let Err(e) = sender.send(()) {
                log::warn!(
                    "Kill signal could not be send to {}, maybe it has already shut down: {e:?}",
                    node.name
                )
            }
        }
    }

    "Network has shut down"
}
