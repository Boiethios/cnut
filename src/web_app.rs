/// The web server allowing to expose an API to the outside world and to display
/// an user interface to monitor the network.

mod endpoints {
    mod node_status;
    pub use node_status::node_status;
    mod static_file;
    pub use static_file::static_file;
    mod stop_start;
    pub use stop_start::stop_start;
}

use crate::{
    error::{Error, Result},
    network::RunningNetwork,
};
use axum::{
    extract::State as AxumState,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use futures::FutureExt;
use std::time::Duration;
use tokio::spawn;

#[derive(Debug, Clone)]
struct AppState {
    network: RunningNetwork,
}

pub async fn serve(network: RunningNetwork) -> Result<()> {
    use endpoints::*;

    let state = AppState { network };

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
        .route("/stop-start", post(stop_start))
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
    state.network.shutdown();

    "Network is shutting down"
}

async fn index() -> Html<&'static str> {
    include_str!("../public/index.html").into()
}

async fn css() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css")],
        include_str!("../public/index.css"),
    )
}

async fn favicon() -> &'static [u8] {
    include_bytes!("../public/favicon.ico")
}
