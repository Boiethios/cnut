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
    error::{Error, Result},
    network::{NodeStatus, RunningNetwork, RunningNode},
    web_app,
};
use std::process::{ExitStatus, Stdio};
use tokio::{process::Command, select, signal, sync::oneshot};

impl RunningNetwork {
    /// Starts all the nodes.
    pub async fn start_all(&self) -> Result<&Self> {
        for node in &self.nodes {
            node.clone().start().await?;
        }

        Ok(self)
    }

    /// Shuts the network down.
    pub async fn stop_all(&self) -> Result<&Self> {
        for node in &self.nodes {
            node.clone().stop().await?;
        }

        Ok(self)
    }

    /// Wait for the network.
    ///
    /// Note that this will prevent any node to be started. Any attempt to do so
    /// will deadlock the call.
    pub async fn wait(&self) -> Result<()> {
        select! {
            _ = signal::ctrl_c() => {},
            _ = self.exit_signal_receiver.lock() => {},
        };

        log::warn!("Network will now shut down");
        for node in self.nodes.iter() {
            let _ = node.stop().await; //TODO
        }

        Ok(())
    }

    /// Serves the web app for debugging, then returns immediately (non-blocking).
    pub async fn serve_web_app(&self) -> Result<()> {
        web_app::serve(self.clone()).await
    }

    /// Serves the web app for debugging, then wait for the network to stop.
    pub async fn serve_web_app_and_wait(&self) -> Result<()> {
        web_app::serve(self.clone()).await?;
        self.wait().await
    }

    /// Returns the node with the given `name`.
    pub fn node_by_name(&self, name: &str) -> Result<&RunningNode> {
        self.nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| Error::NodeNameNotFound(name.to_owned()))
    }

    /// Returns the node with the given `index`.
    pub fn node_by_index(&self, index: usize) -> Result<&RunningNode> {
        self.nodes
            .get(index)
            .ok_or(Error::NodeIndexOutOfBounds(index))
    }
}

impl RunningNode {
    /// Starts the node.
    pub async fn start(&self) -> Result<()> {
        let (kill_sender, kill_receiver) = oneshot::channel();
        let node_path = self.artifact_dir.join("casper-node");
        let config_path = self.data_dir.join("config.toml");
        let mut child = Command::new(&node_path)
            .arg("validator")
            .arg(&config_path)
            .current_dir(&self.data_dir)
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
        tokio::spawn(async move {
            let (result, crash) = tokio::select! {
                exit_result = child.wait() => (exit_result, true), // Early exit (error in the node for example)
                _ = kill_receiver => (child.kill().await.map(|()| ExitStatus::default()), false),
            };

            if let Err(io_err) = result.as_ref() {
                log::warn!("Child process {name:?} has errored: {io_err:?}");
            }
            let status = if crash {
                NodeStatus::Crashed(result)
            } else {
                NodeStatus::Stopped(result)
            };
            (name, status)
        });

        *self.kill_sender.lock().await = Some(kill_sender);
        *self.status.lock().await = NodeStatus::Running;

        Ok(())
    }

    /// Stops the node.
    pub async fn stop(&self) -> Result<()> {
        match self.kill_sender.lock().await.take() {
            Some(kill_sender) => {
                if let Err(()) = kill_sender.send(()) {
                    log::warn!(
                        "Kill signal could not be send to {}, maybe it has already shut down",
                        self.name()
                    )
                }
            }
            None => log::warn!("The node was ordered to get killed, but it is not running"),
        }
        *self.status.lock().await = NodeStatus::Stopped(Ok(ExitStatus::default())); //TODO

        Ok(())
    }

    /// Returns the current status for the node.
    pub async fn status<'a>(&'a self) -> tokio::sync::MutexGuard<'a, NodeStatus> {
        self.status.lock().await
    }
}
