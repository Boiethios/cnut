use std::{
    fmt::{self, write},
    process::Stdio,
};

use crate::{error::Result, network::RunningNode};
use tokio::process::{Child, Command};

#[derive(Debug)]
pub struct NodeProcess {
    child: Option<Child>,
    status: NodeStatus,
}

/// The status of the node.
#[derive(Debug)]
pub enum NodeStatus {
    /// The node is being started.
    Starting,
    /// The node is currently running.
    Running,
    /// The node has stopped because it was killed explicitely.
    Stopped,
    /// The node has crashed.
    Crashed,
}

impl NodeProcess {
    pub async fn start(&mut self, node: &RunningNode) -> Result<()> {
        let node_path = node.artifact_dir().join("casper-node");
        let config_path = node.data_dir().join("config.toml");
        let mut child = Command::new(&node_path)
            .arg("validator")
            .arg(&config_path)
            .current_dir(&node.data_dir())
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

        Ok(())
    }
}

async fn wait_process(child: ) {
    tokio::select! {
        exit_result = child.wait() => (exit_result, true), // Early exit (error in the node for example)
        _ = kill_notifier.notified() => (child.kill().await.map(|()| ExitStatus::default()), false),
    };
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Self::Starting => write!(f, "Starting"),
            Self::Running => write!(f, "Running"),
            Self::Stopped => write!(f, "Stopped"),
            Self::Crashed => write!(f, "Crashed"),
        }
    }
}
