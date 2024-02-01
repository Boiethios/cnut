use crate::web_server::{AppNode, AppState};
use axum::extract::State;
use futures::FutureExt;
use maud::html;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::task::JoinSet;

struct Status {
    name: String,
    running: bool,
    info: Option<LastAddedBlockInfo>,
}

pub async fn node_status(State(state): State<AppState>) -> String {
    match gather_info(&state.nodes).await {
        Err(_) => html! {
            "Error while reading the data"
        },
        Ok(status) => html! {
            table {
                tr {
                    th{"Name"} th{"Era ID"} th{"Height"} th{"Config File"}
                }
                @for status in &status {
                    @let path = format!("/file/{}/config.toml", status.name);
                    //state.nodes[&status.name].path.join("config.toml");
                    tr {
                        td{(status.name)}
                        @if status.running == false {
                            td colspan="2"{"Node not running"}
                        } @else if let Some(info) = status.info.as_ref() {
                            td{(info.era_id)}
                            td{(info.height)}
                        } @else {
                            td{"--"}
                            td{"--"}
                        }
                        td{a .file href=(path) {"config.toml"}}
                    }
                }
            }
        },
    }
    .into()
}

async fn gather_info(nodes: &HashMap<String, AppNode>) -> Result<Vec<Status>, ()> {
    let mut requests = JoinSet::new();
    let mut result = Vec::new();
    let client = Client::new();

    for (name, node) in nodes {
        let name = name.clone();
        requests.spawn(
            client
                .get(format!("http://127.0.0.1:{}/status", node.rest_port))
                .send()
                .map(move |response| (name, response)),
        );
    }

    while let Some(maybe_result) = requests.join_next().await {
        let (name, response) = match maybe_result {
            Err(_) => {
                log::debug!("Could not get the request result from the JoinSet");
                return Err(());
            }
            Ok(data) => data,
        };

        match response {
            Ok(response) => match response.json().await {
                Ok(Payload {
                    last_added_block_info,
                }) => result.push(Status {
                    name,
                    running: true,
                    info: last_added_block_info,
                }),
                Err(e) => {
                    log::debug!("Could not deserialize the node status: {e:?}");
                    return Err(());
                }
            },
            Err(_) => result.push(Status {
                name,
                running: false,
                info: None,
            }),
        }
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(result)
}

#[derive(Deserialize)]
struct Payload {
    last_added_block_info: Option<LastAddedBlockInfo>,
}

#[derive(Deserialize)]
struct LastAddedBlockInfo {
    era_id: usize,
    height: usize,
}
