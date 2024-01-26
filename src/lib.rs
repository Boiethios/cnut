//! See README (maybe we could include the README here).

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations, missing_docs)]
//TODO remove. Temporary noise reduction:
//#![allow(unused_imports, unused_variables)]

pub extern crate tokio;

pub mod artifacts;
pub mod error;
pub mod network;

pub(crate) mod util;

/// Allows to have what is needed to run a network with a single import.
pub mod prelude {
    pub use crate::{
        artifacts::Artifacts,
        network::{Chainspec, Network, Node},
    };
    pub use toml::Value as TomlValue;
}

const PROJECT_NAME: &str = "Utilities for Network Testing";
const PROJECT_DIR: &str = "cnut";
const NODE_GIT_URL: &str = "https://github.com/casper-network/casper-node.git";
