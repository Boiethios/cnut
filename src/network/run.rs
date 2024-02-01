//! The module allowing to run a node. Each node needs the following files to run:
//!
//! - The Casper node binary;
//! - Client smart contracts;
//! - Chainspec:
//!     - `chainspec.toml`;
//!     - `accounts.toml`;
//!     - `global_state.toml`;
//! - Node config: `config.toml`;
//! - The node keys.

use crate::{
    error::Result,
    network::{prepare::prepare_nodes, Network},
    util::crypto::PublicKey,
    web_server::run_server,
};
use std::{
    path::PathBuf,
    process::{ExitStatus, Stdio},
};
use tokio::{process::Command, spawn, sync::oneshot, task::JoinSet};

pub async fn run_network(network: Network) -> Result<()> {
    let root_running_dir = tempfile::tempdir().expect("Failed to create a tmp directory");
    let nodes = prepare_nodes(network, root_running_dir.path()).await?;
    let mut set = JoinSet::new();
    let mut kill_senders = Vec::new();

    for node in nodes.clone() {
        let (sender, receiver) = oneshot::channel();
        kill_senders.push((sender, node.name.clone()));
        set.spawn(node.run(receiver));
    }

    spawn(run_server(
        root_running_dir.path().to_owned(),
        kill_senders,
        nodes,
    ));
    println!("App at http://127.0.0.1:6532");

    while let Some(result) = set.join_next().await {
        let (name, public_key) = result.expect("tokio task has failed")?;

        log::info!("Node {name} ({public_key}) has stopped")
    }

    //println!("Press RETURN");
    //std::io::stdin().read_line(&mut String::new()).unwrap();

    Ok(())
}

/// Internal format more suitable than the public one from the builder.
#[derive(Clone, Debug)]
pub struct RunningNode {
    /// Path where the node will run, with the config.
    pub(crate) running_path: PathBuf,
    /// Path of the directory with binaries (node and wasm).
    pub(crate) bin_path: PathBuf,
    pub(crate) name: String,
    pub(crate) public_key: PublicKey,
    pub(crate) rpc_port: u16,
    pub(crate) rest_port: u16,
    pub(crate) validator: bool,
}

impl RunningNode {
    pub async fn run(self, kill_signal: oneshot::Receiver<()>) -> Result<(String, PublicKey)> {
        let RunningNode {
            running_path,
            bin_path,
            name,
            public_key,
            ..
        } = self;

        let node_path = bin_path.join("casper-node");
        let config_path = running_path.join("config.toml");
        let mut child = Command::new(&node_path)
            .arg("validator")
            .arg(&config_path)
            .current_dir(&running_path)
            // Remove the output:
            .stdout(Stdio::null())
            .spawn()
            .map_err(|io_err| crate::error::Error::FailedToSpawnProcess {
                full_command: format!(
                    "{} validator {}",
                    node_path.to_string_lossy(),
                    config_path.to_string_lossy(),
                ),
                io_err,
            })?;

        log::debug!("Node {name} spawned successfully");

        let _result = tokio::select! {
            exit_result = child.wait() => exit_result, // Normally this branch should never happen
            _ = kill_signal => child.kill().await.map(|()| ExitStatus::default()),
        };

        Ok((name, public_key))
    }
}
