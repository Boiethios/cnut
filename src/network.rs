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

use crate::util::crypto::PublicKey;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    sync::{oneshot, Mutex},
    task::JoinSet,
};

type ProcessExitStatus = std::result::Result<std::process::ExitStatus, std::io::Error>;
type RunningNodeSet = JoinSet<(String, NodeStatus)>;

/// A network representation in CNUT. When this type is obtained, the file tree
/// is created, and it is ready to start, or already started.
#[derive(Clone, Debug)]
pub struct RunningNetwork {
    nodes: Vec<RunningNode>,
    tasks: Arc<Mutex<RunningNodeSet>>,
    base_dir: Arc<tempfile::TempDir>,
}

/// A running node. It can be started, stopped or crashed.
///
/// The mutable data is shared behind a counted reference, so cloned nodes
/// hold the same values.
#[derive(Clone, Debug)]
pub struct RunningNode {
    /// Path where the node will run, with the config.
    running_path: PathBuf,
    /// Path of the directory with binaries (node and wasm).
    bin_path: PathBuf,

    name: String,
    public_key: PublicKey,
    validator: bool,

    rpc_port: u16,
    rest_port: u16,
    speculative_execution_port: u16,

    status: Arc<Mutex<NodeStatus>>,
    pub(crate) kill_sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
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
}

impl RunningNode {
    /// Returns the node name.
    pub fn name(&self) -> &str {
        &self.name
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
    pub fn running_path(&self) -> &Path {
        &self.running_path
    }

    /// Path of the directory with binaries (node and wasm).
    pub fn bin_path(&self) -> &Path {
        &self.bin_path
    }

    /// Chainspec path.
    pub fn chainspec_path(&self) -> PathBuf {
        self.running_path().join("chainspec.toml")
    }

    /// Configuration path.
    pub fn config_path(&self) -> PathBuf {
        self.running_path().join("config.toml")
    }

    /// Secret key path.
    pub fn secret_key_path(&self) -> PathBuf {
        self.running_path().join("secret_key.pem")
    }
}
