use crate::{artifacts::Artifacts, error::Result};
use sealed::NetworkItem;
use std::{ops, path::PathBuf};

/// The notwork. Add the nodes, and run it.
#[derive(Debug, Clone)]
pub struct Network {
    pub(crate) nodes: Vec<Node>,
    /// Chainspec for the nodes. If it is not specified, the one from the first
    /// available node with be taken.
    pub(crate) chainspec: Option<Chainspec>,
}

mod sealed {
    pub trait NetworkItem {
        fn add_to(self, network: &mut super::Network);
    }
}

impl Network {
    /// Creates a new `Network`.
    pub fn new() -> Self {
        Network {
            nodes: Vec::new(),
            chainspec: None,
        }
    }

    /// Adds a new item to the network. See [`NetworkItem`].
    pub fn with(mut self, item: impl NetworkItem) -> Self {
        item.add_to(&mut self);
        self
    }

    /// Returns the chainspec's full path.
    ///
    /// If it is not explicitely specified, we use the first node template one.
    pub(crate) fn chainspec(&self) -> PathBuf {
        self.chainspec
            .clone()
            .unwrap_or_else(|| Chainspec::Artifacts(self.nodes.first().unwrap().artifacts.clone()))
            .path()
    }

    /// Runs the network.
    pub async fn run(self) -> Result<()> {
        super::run_network(self).await
    }

    /// Returns the amount of nodes in the network.
    pub(crate) fn amount_nodes(&self) -> usize {
        self.nodes.iter().map(|n| n.amount).sum()
    }
}

/// Several nodes to be added, with the given artifacts.
#[derive(Debug, Clone)]
pub struct Node {
    pub(crate) artifacts: Artifacts,
    pub(crate) amount: usize,
    /// Overload the config from `Artifacts`.
    pub(crate) config: Option<NodeConfig>,
    pub(crate) name: Option<String>,
    pub(crate) validator: bool,
}

/// Where to find the chainspec for the network.
#[derive(Debug, Clone)]
pub enum Chainspec {
    /// A path for the chainspec: `some/dir/Chainspec.toml`.
    Path(PathBuf),
    /// An [`Artifacts`] structure.
    Artifacts(Artifacts),
}

/// Where to find the node configuration.
#[derive(Debug, Clone)]
pub enum NodeConfig {
    /// A path for the chainspec: `some/dir/Chainspec.toml`.
    Path(PathBuf),
    /// An [`Artifacts`] structure.
    Artifacts(Artifacts),
}

// Node

impl NetworkItem for Node {
    fn add_to(self, network: &mut Network) {
        network.nodes.push(self);
    }
}

impl Node {
    /// Creates a new validator [`Node`] from [`Artifacts`].
    pub fn validator(artifacts: Artifacts) -> Self {
        Self {
            artifacts,
            amount: 1,
            config: None,
            name: None,
            validator: true,
        }
    }

    /// Creates a new non-validator [`Node`] from [`Artifacts`].
    pub fn keep_up(artifacts: Artifacts) -> Self {
        Self {
            artifacts,
            amount: 1,
            config: None,
            name: None,
            validator: false,
        }
    }

    /// Overloads the config for this node or these nodes.
    pub fn config(self, config: impl Into<NodeConfig>) -> Self {
        Self {
            config: Some(config.into()),
            ..self
        }
    }

    /// Overloads the config for this node or these nodes.
    pub fn name(self, name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..self
        }
    }
}

impl ops::Mul<Node> for usize {
    type Output = Node;

    fn mul(self, rhs: Node) -> Self::Output {
        Node {
            amount: self * rhs.amount,
            ..rhs
        }
    }
}

impl From<Artifacts> for NodeConfig {
    fn from(artifacts: Artifacts) -> Self {
        NodeConfig::Artifacts(artifacts)
    }
}

impl<P: Into<PathBuf>> From<P> for NodeConfig {
    fn from(path: P) -> Self {
        NodeConfig::Path(path.into())
    }
}

impl NodeConfig {
    pub(crate) fn path(&self) -> PathBuf {
        match self {
            Self::Path(path) => path.to_owned(),
            Self::Artifacts(artifacts) => artifacts.config_path(),
        }
    }
}

// Chainspec

impl Chainspec {
    pub(crate) fn path(&self) -> PathBuf {
        match self {
            Self::Path(path) => path.clone(),
            Self::Artifacts(artifacts) => artifacts.chainspec_path(),
        }
    }
}

impl NetworkItem for Chainspec {
    fn add_to(self, network: &mut Network) {
        network.chainspec = Some(self);
    }
}

impl From<Artifacts> for Chainspec {
    fn from(artifacts: Artifacts) -> Self {
        Chainspec::Artifacts(artifacts)
    }
}

impl<P: Into<PathBuf>> From<P> for Chainspec {
    fn from(path: P) -> Self {
        Chainspec::Path(path.into())
    }
}
