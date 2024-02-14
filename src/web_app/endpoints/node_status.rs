use crate::{network::RunningNode, web_app::AppState};
use axum::extract::State;
use maud::html;
use reqwest::Client;
use serde::Deserialize;
use tokio::task::JoinSet;

struct Status {
    name: String,
    validator: bool,
    running: bool,
    info: Option<LastAddedBlockInfo>,
}

pub async fn node_status(State(state): State<AppState>) -> String {
    match gather_info(&state.network.nodes).await {
        Err(_) => html! {
            "Error while reading the data"
        },
        Ok(status) => html! {
            table {
                tr {
                    th{"Name"} th{"Era ID"} th{"Height"} th{"Validator"} th{"Config File"} th{"Stop/Start"}
                }
                @for status in &status {
                    @let path = format!("/file/{}/config.toml", status.name);
                    @let stop_start = format!("/stop-start?name={}", status.name);
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
                        td{ @if status.validator { "Yes" } @else { "No" } }
                        td{a .file href=(path) {"config.toml"}}
                        td{@if status.running {
                            button class="red" hx-post=(stop_start) {"Stop"}
                        } @else {
                            button class="green" hx-post=(stop_start) {"Start"}
                        }}
                    }
                }
            }
        },
    }
    .into()
}

async fn gather_info(nodes: &[RunningNode]) -> Result<Vec<Status>, ()> {
    let mut requests = JoinSet::new();
    let client = Client::new();

    for node in nodes {
        let name = node.name().to_owned();
        let validator = node.validator();
        let request = client
            .get(format!("http://127.0.0.1:{}/status", node.rest_port()))
            .send();
        requests.spawn(async move {
            match request.await {
                Ok(response) => match response.json().await {
                    Ok(Payload {
                        last_added_block_info,
                    }) => Ok(Status {
                        name,
                        validator,
                        running: true,
                        info: last_added_block_info,
                    }),
                    Err(e) => {
                        log::debug!("Could not deserialize the node status: {e:?}");
                        return Err(());
                    }
                },
                Err(_) => Ok(Status {
                    name,
                    validator,
                    running: false,
                    info: None,
                }),
            }
        });
    }

    let mut result = Vec::new();
    while let Some(maybe_result) = requests.join_next().await {
        match maybe_result {
            Err(_) => {
                log::debug!("Could not get the request result from the JoinSet");
                return Err(());
            }
            Ok(maybe_data) => result.push(maybe_data?),
        };
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
