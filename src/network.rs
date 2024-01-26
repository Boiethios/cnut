//! Provides what is needed to run a network.

mod builder;
mod run;
mod web_server;

pub(crate) use builder::NodeConfig;
pub use builder::{Chainspec, Network, Node};
pub(crate) use run::{run_network, RunningNode};
pub(crate) use web_server::run_server;
