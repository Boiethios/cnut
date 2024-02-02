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
pub use prepare::PreparedNetwork;
pub use run::RunningNetwork;

pub(crate) use describe::NodeConfig;
pub(crate) use prepare::{prepare_network, PreparedNode};
pub(crate) use run::{run_network, RunningNode};
