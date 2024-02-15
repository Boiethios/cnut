use crate::web_app::AppState;
use axum::extract::{Query, State};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Named {
    name: String,
}

pub async fn stop_start(
    State(mut state): State<AppState>,
    Query(Named { name }): Query<Named>,
) -> Result<(), &'static str> {
    log::trace!("stop_start endpoint");
    let node = state
        .network
        .nodes
        .iter_mut()
        .find(|node| node.name() == name)
        .ok_or("Unknown node name")
        .inspect_err(|_| log::warn!("Unknown node name: {name}"))?;

    if node.running().await {
        log::debug!("Node {name} is asked to STOP");
        node.stop().await.map_err(|_| "Cannot stop the node")?;
    } else {
        log::debug!("Node {name} is asked to START");
        node.start().await.map_err(|_| "Cannot start the node")?;
    }

    Ok(())
}
