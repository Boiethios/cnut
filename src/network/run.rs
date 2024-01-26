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
    artifacts::Artifacts,
    error::{Error, Result},
    network::Network,
    util::{
        crypto::{self, generate_pair, PublicKey},
        map, update_toml, LettersGen, Spinner,
    },
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    str::FromStr as _,
};
use tokio::{fs, process::Command, spawn, sync::oneshot, task::JoinSet};

use super::run_server;

const BASE_BIND_ADDRESS: u16 = 34000;
const BASE_RPC_ADDRESS: u16 = 7777;
const BASE_SPEC_ADDRESS: u16 = 6666;
const BASE_REST_ADDRESS: u16 = 8888;
const BASE_EVENT_STREAM_ADDRESS: u16 = 9999;

pub async fn run_network(network: Network) -> Result<()> {
    let root_running_dir = tempfile::tempdir().expect("Failed to create a tmp directory");
    let nodes = prepare_nodes(network, root_running_dir.path()).await?;
    let mut set = JoinSet::new();
    let mut kill_senders = Vec::new();

    for node in nodes.clone() {
        let (sender, receiver) = oneshot::channel();
        kill_senders.push((sender, node.name.clone()));
        set.spawn(node.run(receiver));
    }

    spawn(run_server(kill_senders, nodes));
    println!("App at http://127.0.0.1:6532");

    while let Some(result) = set.join_next().await {
        let (name, public_key) = result.expect("tokio task has failed")?;

        log::info!("Node {name} ({public_key}) has stopped successfully.")
    }

    Ok(())
}

/// Internal format more suitable than the public one from the builder.
#[derive(Clone)]
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
}

impl RunningNode {
    pub async fn run(self, kill_signal: oneshot::Receiver<()>) -> Result<(String, PublicKey)> {
        let RunningNode {
            running_path,
            bin_path,
            name,
            public_key,
            ..
        } = self;

        let node_path = bin_path.join("casper-node");
        let config_path = running_path.join("config.toml");
        let mut child = Command::new(&node_path)
            .arg("validator")
            .arg(&config_path)
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

        log::debug!("Node {name} spawned successfully");

        let _result = tokio::select! {
            exit_result = child.wait() => exit_result, // Normally this branch should never happen
            _ = kill_signal => child.kill().await.map(|()| ExitStatus::default()),
        };

        Ok((name, public_key))
    }
}

async fn prepare_nodes(network: Network, root_running_dir: &Path) -> Result<Vec<RunningNode>> {
    let mut nodes = Vec::new();
    let mut conf_names = LettersGen::new();
    let rng = &mut rand::thread_rng();
    let chainspec_path = root_running_dir.join("chainspec.toml");
    let accounts_path = root_running_dir.join("accounts.toml");
    let spinner = Spinner::create("Preparing the node files");

    log::debug!("Running dir created at: {:?}", root_running_dir);
    println!("Running dir created at: {:?}", root_running_dir);

    write_chainspec(
        network.chainspec(),
        &chainspec_path,
        map!(&["protocol", "activation_point"][..] => 0.into()),
    )
    .await?;

    // Create an empty accounts file to be able to link to (the hardlink call fails otherwise):
    fs::write(&accounts_path, "")
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("creating the accounts file {accounts_path:?}"),
            io_err,
        })?;

    let known_addresses: Vec<_> = (BASE_BIND_ADDRESS
        ..BASE_BIND_ADDRESS + network.amount_nodes() as u16)
        .map(|i| toml::Value::from(format!("127.0.0.1:{i}")))
        .collect();
    let mut index = 0u16..;

    //TODO parallelize
    for super::Node {
        artifacts,
        amount,
        config,
        name,
        validator,
    } in network.nodes
    {
        let name = name.unwrap_or_else(|| format!("Node_{}", conf_names.next()));
        // `<temp dir>/<node name>`
        let config_path = node_config(config, artifacts.clone());

        let node_paths_and_names = match amount {
            0 => vec![],
            1 => vec![(root_running_dir.join(&name), name)],
            n => (0..n)
                .map(|i| {
                    (
                        root_running_dir.join(&name).join(format!("{i}")),
                        format!("{name}/{i}"),
                    )
                })
                .collect(),
        };

        for (running_path, name) in node_paths_and_names {
            let (public_key, secret_key) = generate_pair(rng);

            // Create the directory:
            fs::create_dir_all(&running_path)
                .await
                .map_err(|io_err| Error::FileOperation {
                    description: format!("cannot create the folder {running_path:?}"),
                    io_err,
                })?;

            // Copy the config to the running path:
            let dest = running_path.join("config.toml");
            fs::copy(&config_path, &dest)
                .await
                .map_err(|io_err| Error::FileOperation {
                    description: format!("cannot copy the config file {config_path:?} to {dest:?}"),
                    io_err,
                })?;
            let index = index.next().unwrap();
            let rpc_port = BASE_RPC_ADDRESS + index;
            let rest_port = BASE_REST_ADDRESS + index;
            write_config(
                &config_path,
                running_path.join("config.toml"),
                map! {
                    ["network", "bind_address"].as_ref() => format!("0.0.0.0:{}", BASE_BIND_ADDRESS + index).into(),
                    ["network", "known_addresses"].as_ref() => known_addresses.clone().into(),
                    ["rpc_server", "address"].as_ref() => format!("0.0.0.0:{rpc_port}").into(),
                    ["speculative_exec_server", "address"].as_ref() => format!("0.0.0.0:{}", BASE_SPEC_ADDRESS + index).into(),
                    ["rest_server", "address"].as_ref() => format!("0.0.0.0:{rest_port}").into(),
                    ["event_stream_server", "address"].as_ref() => format!("0.0.0.0:{}", BASE_EVENT_STREAM_ADDRESS + index).into(),
                },
            )
            .await?;

            // Create the `pem` file:
            crypto::write_pem(&secret_key, running_path.join("secret_key.pem")).await?;

            // Link the chainspec (including the accounts):
            let dest = running_path.join("chainspec.toml");
            fs::hard_link(&chainspec_path, &dest)
                .await
                .map_err(|io_err| Error::FileOperation {
                    description: format!(
                        "hard-linking the chainspec {chainspec_path:?} to {dest:?}"
                    ),
                    io_err,
                })?;
            let dest = running_path.join("accounts.toml");
            fs::hard_link(&accounts_path, &dest)
                .await
                .map_err(|io_err| Error::FileOperation {
                    description: format!("hard-linking the accounts {accounts_path:?} to {dest:?}"),
                    io_err,
                })?;

            nodes.push(RunningNode {
                running_path,
                bin_path: artifacts.0.clone(),
                name,
                public_key,
                rpc_port,
                rest_port,
                validator,
            });
        }
    }

    // Create the `accounts.toml` file to the root:
    fs::write(
        &accounts_path,
        toml::to_string_pretty(&accounts(&nodes)).expect("TOML serialization failed"),
    )
    .await
    .map_err(|io_err| Error::FileOperation {
        description: format!("writing the chainspec accounts {accounts_path:?}"),
        io_err,
    })?;

    spinner.success();
    Ok(nodes)
}

/// Returns a TOML data structure with the accounts.
fn accounts(nodes: &[RunningNode]) -> toml::Value {
    use toml::{map::Map, Value};

    let accounts = nodes
        .iter()
        .map(|node| {
            let mut map = Map::new();
            map.insert("public_key".to_owned(), node.public_key.to_string().into());
            map.insert(
                "balance".to_owned(),
                "1000000000000000000000000000".to_owned().into(),
            );
            if node.validator {
                map.insert("validator".to_owned(), {
                    let mut map = Map::new();
                    map.insert(
                        "bonded_amount".to_owned(),
                        "500000000000000".to_owned().into(),
                    );
                    map.into()
                });
            }
            Value::Table(map)
        })
        .collect();

    let accounts = {
        let mut map = Map::new();
        map.insert("accounts".to_owned(), Value::Array(accounts));
        map
    };

    Value::Table(accounts)
}

async fn write_chainspec(
    src: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    updates: HashMap<&[&str], toml::Value>,
) -> Result<()> {
    let (src, dest) = (src.as_ref(), dest.as_ref());

    log::debug!("Copying the chainspec from {src:?} to {dest:?}");

    let chainspec = fs::read_to_string(src)
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("reading the chainspec {src:?}"),
            io_err,
        })?;
    let chainspec = toml::Value::from_str(&chainspec)?;

    fs::write(
        dest,
        toml::to_string_pretty(&update_toml(chainspec, updates)?)
            .expect("TOML serialization failed"),
    )
    .await
    .map_err(|io_err| Error::FileOperation {
        description: format!("writing the chainspec {dest:?}"),
        io_err,
    })?;

    Ok(())
}

async fn write_config(
    src: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    updates: HashMap<&[&str], toml::Value>,
) -> Result<()> {
    let (src, dest) = (src.as_ref(), dest.as_ref());

    log::debug!("Copying the config from {src:?} to {dest:?}");

    let config = fs::read_to_string(src)
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("reading the config {src:?}"),
            io_err,
        })?;
    let config = toml::Value::from_str(&config)?;

    fs::write(
        dest,
        toml::to_string_pretty(&update_toml(config, updates)?).expect("TOML serialization failed"),
    )
    .await
    .map_err(|io_err| Error::FileOperation {
        description: format!("writing the config {dest:?}"),
        io_err,
    })?;

    Ok(())
}

fn node_config(config: Option<super::NodeConfig>, artifacts: Artifacts) -> PathBuf {
    match config.unwrap_or_else(|| super::NodeConfig::Artifacts(artifacts)) {
        super::NodeConfig::Path(path) => path,
        super::NodeConfig::Artifacts(artifacts) => artifacts.0.join("config.toml"),
    }
}
