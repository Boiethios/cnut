//! Provides what is needed to run a network.
//!
//! This goes in 3 steps:
//!
//! - First the network must be described: how many nodes, which configs,
//! which chainspec, etc.
//! At this step, the type used is [`NetworkBuilder`].
//! - Then the network must be prepared: the needed files are copied into a tree
//! structure ready to be run.
//! At this step, the type used is [`PreparedNetwork`].
//! - Finally, the node can be run. A [`RunningNetwork`] is then returned.

mod describe;
mod prepare;
mod run;

pub use describe::{Chainspec, NetworkBuilder, Node};

pub(crate) use describe::NodeConfig;
pub(crate) use prepare::prepare_network;

use crate::util::{
    crypto::{PublicKey, SecretKey},
    ShutdownState,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, AtomicU64},
        Arc,
    },
};
use tokio::{
    process::Child,
    sync::{Mutex, Notify},
};
use tokio_util::task::task_tracker::TaskTracker;

type ProcessExitStatus = std::result::Result<std::process::ExitStatus, std::io::Error>;

/// A network representation in CNUT. When this type is obtained, the file tree
/// is created, and it is ready to start, or already started.
#[derive(Clone, Debug)]
pub struct RunningNetwork {
    pub(crate) nodes: Vec<RunningNode>,
    temp_directory: Arc<tempfile::TempDir>,
    shutdown_state: ShutdownState,
    exit_notification: Arc<Notify>,
    task_tracker: TaskTracker,
}

/// A running node. It can be started, stopped or crashed.
///
/// The mutable data is shared behind a counted reference, so cloned nodes
/// hold the same values.
#[derive(Clone, Debug)]
pub struct RunningNode {
    /// Path where the node will run, with the config.
    data_dir: PathBuf,
    /// Path of the directory with binaries (node and wasm).
    artifact_dir: PathBuf,
    /// Used during the node preparation phase.
    default_config_path: PathBuf,

    name: String,
    public_key: PublicKey,
    secret_key: SecretKey,
    validator: bool,

    rpc_port: u16,
    rest_port: u16,
    speculative_execution_port: u16,

    process_id: Arc<AtomicU32>,
    task_tracker: TaskTracker,
    status: Arc<Mutex<NodeStatus>>,
    pub(crate) kill_notifier: Arc<Notify>,
}

/// The status of the node.
#[derive(Debug)]
pub enum NodeStatus {
    /// The node is currently running.
    Running,
    /// The node has stopped because it was killed explicitely.
    Stopped(ProcessExitStatus),
    /// The node has crashed.
    Crashed(ProcessExitStatus),
}

impl Default for NodeStatus {
    fn default() -> Self {
        NodeStatus::Stopped(Ok(std::process::ExitStatus::default()))
    }
}

impl RunningNetwork {
    /// Returns the number of nodes in the network.
    pub fn nodes_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the directory where all the data is located in.
    pub fn temp_directory(&self) -> &Path {
        self.temp_directory.path()
    }

    /// Orders the network to shutdown. This causes the wait functions to return.
    pub fn shutdown(&self) {
        self.exit_notification.notify_one();
    }
}

impl RunningNode {
    /// Returns the node name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns weither the node is a validator or not.
    pub fn validator(&self) -> bool {
        self.validator
    }

    /// Returns if the node is running.
    pub async fn running(&self) -> bool {
        self.status.lock().await.running()
    }

    /// Returns the RPC port for this node.
    pub fn rpc_port(&self) -> u16 {
        self.rpc_port
    }

    /// Returns the REST port for this node.
    pub fn rest_port(&self) -> u16 {
        self.rest_port
    }

    /// Returns the speculative execution port for this node.
    pub fn speculative_execution_port(&self) -> u16 {
        self.speculative_execution_port
    }

    /// Path where the node will run, with the config, secret key, chainspec, etc.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Path of the directory with binaries (node and wasm).
    pub fn artifact_dir(&self) -> &Path {
        &self.artifact_dir
    }

    /// Chainspec path.
    pub fn chainspec_path(&self) -> PathBuf {
        self.data_dir().join("chainspec.toml")
    }

    /// Configuration path.
    pub fn config_path(&self) -> PathBuf {
        self.data_dir().join("config.toml")
    }

    /// Secret key path.
    pub fn secret_key_path(&self) -> PathBuf {
        self.data_dir().join("secret_key.pem")
    }

    /// Public key path.
    pub fn public_key_path(&self) -> PathBuf {
        self.data_dir().join("public_key.pem")
    }
}

impl NodeStatus {
    fn running(&self) -> bool {
        matches!(self, Self::Running)
    }
}
