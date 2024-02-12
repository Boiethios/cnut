//! This modules creates a tree of files allowing to run the nodes. It mainly
//! consists of configuration and filesystem operations.

use crate::{
    error::{Error, Result},
    network::{NetworkBuilder, RunningNetwork, RunningNode},
    util::{
        crypto::{self, generate_pair},
        toml_map, update_toml, LettersGen, Spinner,
    },
};
use std::{
    path::Path,
    str::FromStr as _,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tempfile::TempDir;
use tokio::fs;

pub async fn prepare_network(network: NetworkBuilder) -> Result<RunningNetwork> {
    let base_dir = create_temp_dir()?;
    let root_running_dir = base_dir.path();
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
        toml_map! {
            "core", "validator_slots" => network.amount_nodes() as i64,
            "protocol", "activation_point" => millis_from_now(1000),
            "protocol", "version" => "1.0.0",
        },
    )
    .await?;

    // Create an empty accounts file to be able to link to (the hardlink call fails otherwise):
    fs::write(&accounts_path, "")
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("creating the accounts file {accounts_path:?}"),
            io_err,
        })?;

    let known_addresses: Vec<_> = (port::bind(0)..port::bind(network.amount_nodes()))
        .map(|i| toml::Value::from(format!("127.0.0.1:{i}")))
        .collect();
    let mut index = 0..;

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
        let config_path = config
            .unwrap_or_else(|| super::NodeConfig::Artifacts(artifacts.clone()))
            .path();

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

        for (running_path, name) in node_paths_and_names.into_iter() {
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
            let rpc_port = port::rpc(index);
            let rest_port = port::rest(index);
            let speculative_execution_port = port::spec(index);
            write_config(
                &config_path,
                running_path.join("config.toml"),
                toml_map! {
                    "network", "bind_address" => format!("0.0.0.0:{}", port::bind(index)),
                    "network", "known_addresses" => known_addresses.clone(),
                    "rpc_server", "address" => format!("0.0.0.0:{rpc_port}"),
                    "speculative_exec_server", "address" => format!("0.0.0.0:{}", speculative_execution_port),
                    "rest_server", "address" => format!("0.0.0.0:{rest_port}"),
                    "event_stream_server", "address" => format!("0.0.0.0:{}", port::event_stream(index)),
                    "storage", "path" => "./node-storage",
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
                validator,
                rpc_port,
                rest_port,
                speculative_execution_port,
                status: Default::default(),
                kill_sender: Default::default(),
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

    Ok(RunningNetwork {
        base_dir,
        nodes,
        tasks: Default::default(),
    })
}

async fn write_chainspec(
    src: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    updates: toml::Table,
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
        toml::to_string_pretty(&update_toml(chainspec, updates))
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
    updates: toml::Table,
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
        toml::to_string_pretty(&update_toml(config, updates)).expect("TOML serialization failed"),
    )
    .await
    .map_err(|io_err| Error::FileOperation {
        description: format!("writing the config {dest:?}"),
        io_err,
    })?;

    Ok(())
}

/// A timestamp of the moment in `n` seconds from now.
fn millis_from_now(n: u64) -> String {
    let value = SystemTime::now() + Duration::from_millis(n);

    humantime::format_rfc3339_millis(value).to_string()
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

fn create_temp_dir() -> Result<Arc<TempDir>> {
    let temp_dir = Arc::new(tempfile::tempdir().map_err(|io_err| Error::FileOperation {
        description: format!("creating the temporary directory"),
        io_err,
    })?);

    Ok(temp_dir)
}

mod port {
    const BASE_BIND_ADDRESS: u16 = 34000;
    const BASE_SPEC_ADDRESS: u16 = 6666;
    const BASE_RPC_ADDRESS: u16 = 7777;
    const BASE_REST_ADDRESS: u16 = 8888;
    const BASE_EVENT_STREAM_ADDRESS: u16 = 9999;

    pub fn bind(index: usize) -> u16 {
        BASE_BIND_ADDRESS + index as u16
    }

    pub fn spec(index: usize) -> u16 {
        BASE_SPEC_ADDRESS + index as u16
    }

    pub fn rpc(index: usize) -> u16 {
        BASE_RPC_ADDRESS + index as u16
    }

    pub fn rest(index: usize) -> u16 {
        BASE_REST_ADDRESS + index as u16
    }

    pub fn event_stream(index: usize) -> u16 {
        BASE_EVENT_STREAM_ADDRESS + index as u16
    }
}
