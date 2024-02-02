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
    network::{PreparedNetwork, PreparedNode},
    util::crypto::PublicKey,
    web_app,
};
use std::{
    path::PathBuf,
    process::{ExitStatus, Stdio},
    rc::Rc,
    sync::Arc,
};
use tokio::{
    process::Command,
    sync::{oneshot, Mutex},
    task::JoinSet,
};

type RunningNodeSet = JoinSet<(String, PublicKey)>;

///TODO
#[derive(Debug)]
pub struct RunningNetwork {
    pub(crate) nodes: Vec<RunningNode>,
    pub(crate) tasks: RunningNodeSet,
    pub(crate) base_dir: Rc<tempfile::TempDir>,
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

    pub(crate) kill_sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

pub async fn run_network(network: PreparedNetwork) -> Result<RunningNetwork> {
    let PreparedNetwork { nodes, base_dir } = network;
    let mut tasks = JoinSet::new();
    let mut nodes: Vec<_> = nodes.clone().into_iter().map(RunningNode::from).collect();

    for node in &mut nodes {
        node.run(&mut tasks).await?;
    }

    let network = RunningNetwork {
        nodes,
        tasks,
        base_dir,
    };

    Ok(network)
}

impl RunningNetwork {
    /// Wait for the network to stop.
    pub async fn wait(mut self) -> Result<()> {
        while let Some(result) = self.tasks.join_next().await {
            match result {
                Ok((name, public_key)) => log::info!("Node {name} ({public_key}) has stopped"),
                Err(io_err) => log::warn!("Failed to join the process task: {io_err:?}"),
            }
        }

        Ok(())
    }

    /// Serves the web app for debugging
    pub async fn serve_web_app_and_wait(self) -> Result<()> {
        web_app::serve(self.nodes.clone(), self.base_dir.path().to_owned()).await?;
        self.wait().await
    }

    /// Returns the node with the given `name`.
    pub fn node_by_name(&self, name: &str) -> Option<&RunningNode> {
        self.nodes.iter().find(|node| node.name == name)
    }

    /// Returns the node with the given `index`.
    pub fn node_by_index(&self, index: usize) -> Option<&RunningNode> {
        self.nodes.get(index)
    }
}

impl RunningNode {
    async fn run(&mut self, set: &mut RunningNodeSet) -> Result<()> {
        let (kill_sender, kill_receiver) = oneshot::channel();
        let node_path = self.bin_path.join("casper-node");
        let config_path = self.running_path.join("config.toml");
        let mut child = Command::new(&node_path)
            .arg("validator")
            .arg(&config_path)
            .current_dir(&self.running_path)
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

        log::debug!("Node {} spawned successfully", self.name);

        let name = self.name.clone();
        let public_key = self.public_key.clone();
        set.spawn(async move {
            let result = tokio::select! {
                exit_result = child.wait() => exit_result, // Normally this branch should never happen
                _ = kill_receiver => child.kill().await.map(|()| ExitStatus::default()),
            };

            if let Err(io_err) = result {
                log::warn!("Child process {name:?} has errored: {io_err:?}");
            }
            (name, public_key)
        });

        let mut locked = self.kill_sender.lock().await;
        *locked = Some(kill_sender);

        Ok(())
    }
}

impl From<PreparedNode> for RunningNode {
    fn from(
        PreparedNode {
            running_path,
            bin_path,
            name,
            public_key,
            rpc_port,
            rest_port,
            validator,
        }: PreparedNode,
    ) -> Self {
        RunningNode {
            running_path,
            bin_path,
            name,
            public_key,
            rpc_port,
            rest_port,
            validator,
            kill_sender: Arc::new(Mutex::new(None)),
        }
    }
}
