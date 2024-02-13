use crate::web_app::AppState;
use axum::extract::{Query, State};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Named {
    name: String,
}

pub async fn stop_start(
    State(state): State<AppState>,
    Query(Named { name }): Query<Named>,
) -> Result<(), &'static str> {
    let node = state
        .network
        .nodes
        .iter()
        .find(|node| node.name() == name)
        .ok_or("Unknown node name")?;

    if node.running().await {
        // STOP
        let kill_sender = node
            .kill_sender
            .lock()
            .await
            .take()
            .ok_or("No killer handle")?;
        kill_sender
            .send(())
            .map_err(|_| "Cannot send the kill signal")?;
    } else {
        // START
        state
            .network
            .start(&name)
            .await
            .map_err(|_| "Cannot start the network")?;
        log::debug!("Node {name} started");
    }

    Ok(())
}
