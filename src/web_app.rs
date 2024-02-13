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
    response::Html,
    routing::{get, post},
    Router,
};
use futures::FutureExt;
use std::{path::PathBuf, time::Duration};
use tokio::spawn;

#[derive(Debug, Clone)]
struct AppState {
    nodes: Vec<RunningNode>,
    base_dir: PathBuf,
}

pub async fn serve(nodes: Vec<RunningNode>, base_dir: PathBuf) -> Result<()> {
    use endpoints::*;

    let state = AppState { nodes, base_dir };

    let app = Router::new()
        .route("/", get(index))
        .route("/index.css", get(css))
        .route("/favicon.ico", get(favicon))
        .nest(
            "/file",
            Router::new().route("/*path", get(endpoints::static_file)),
        )
        .route("/node-status", get(node_status))
        .route("/shutdown", post(shutdown))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:6532").await.unwrap();

    let handle = spawn(async move {
        axum::serve(listener, app).await.map_err(|e| {
            log::error!("Monitoring web server crashed: {e:?}");
            Error::StartingServerWeb(e)
        })
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    if let Some(Ok(result)) = handle.now_or_never() {
        result?;
    }

    println!("Web app at http://127.0.0.1:6532");
    Ok(())
}

async fn shutdown(AxumState(state): AxumState<AppState>) -> &'static str {
    log::debug!("Kill all nodes signal sent");
    for node in &state.nodes {
        let _ = node.kill().await;
    }

    "Network has shut down"
}

async fn index() -> Html<&'static str> {
    include_str!("../public/index.html").into()
}

async fn css() -> &'static str {
    include_str!("../public/index.css")
}

async fn favicon() -> &'static [u8] {
    include_bytes!("../public/favicon.ico")
}
