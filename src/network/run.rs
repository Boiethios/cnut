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
use std::{
    process::{ExitStatus, Stdio},
    sync::Arc,
};
use tokio::{process::Command, select, signal, sync::mpsc};

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
            _ = signal::ctrl_c() => {log::debug!("Got CTRL+C signal, shutting down")},
            _ = self.exit_notification.notified() => {log::debug!("Got a shutting down order")},
            _ = self.task_tracker.wait() => {log::debug!("No node is running anymore")},
        };

        clean_kill_all(self).await;

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
    pub async fn start(&mut self) -> Result<()> {
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

        log::info!("Node {} spawned successfully", self.name);

        let name = self.name.clone();
        let kill_notifier = self.kill_notifier.clone();
        let pid = child.id().unwrap_or_default();
        self.task_tracker.spawn(async move {
            let (result, crash) = tokio::select! {
                exit_result = child.wait() => (exit_result, true), // Early exit (error in the node for example)
                _ = kill_notifier.notified() => (child.kill().await.map(|()| ExitStatus::default()), false),
            };
            log::info!("Child process {name:?} has stopped: {result:?}. Crashed: {crash}");

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

        self.process_id
            .store(pid, std::sync::atomic::Ordering::Relaxed);
        *self.status.lock().await = NodeStatus::Running;

        Ok(())
    }

    /// Stops the node.
    pub async fn stop(&mut self) -> Result<()> {
        self.kill_process()?;
        self.process_id
            .store(0, std::sync::atomic::Ordering::Relaxed);
        *self.status.lock().await = NodeStatus::Stopped(Ok(ExitStatus::default()));

        Ok(())
    }

    /// Returns `true` if the process could be killed (*i.e.* it is not running).
    pub(crate) fn kill_process(&mut self) -> Result<()> {
        self.kill_notifier.notify_one();

        Ok(())
    }

    /// Returns the current status for the node.
    pub async fn status<'a>(&'a self) -> tokio::sync::MutexGuard<'a, NodeStatus> {
        self.status.lock().await
    }
}

impl Drop for RunningNetwork {
    fn drop(&mut self) {
        // If the network has not been shut down correctly,
        // kill all the hard way:
        if self.shutdown_state.must_shut_down() {
            hard_kill_all(self);
        }
    }
}

/// Set the network as shutting down and ask all the processes to stop.
async fn clean_kill_all(network: &RunningNetwork) {
    log::info!("Network will now shut down");

    //TODO verify that the network isn't already shutting down

    for mut node in network.nodes.iter().map(Clone::clone) {
        let _ = node.stop().await;
    }
}

/// Set the network as shutting down and force kill all the processes.
fn hard_kill_all(network: &RunningNetwork) {
    log::info!("Network will now shut down");

    for mut node in network.nodes.iter().map(Clone::clone) {
        if node.process_id.load(std::sync::atomic::Ordering::Relaxed) != 0 {
            //
        }
    }
}
