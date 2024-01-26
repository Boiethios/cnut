//! Handles everything needed to run a node: the node binary and the various
//! WASM contracts and configuration files needed for it to run. See
//! [`Artifacts`] for more detail.

use crate::{
    error::{Error, ProcessError, Result},
    util::{spawn_process, ProcessOutputExt as _, Spinner},
};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};
use tokio::fs;

// 1. Location.
// 2. Compilation?
// 3. Binary path

/// The following files are included in the `Artifacts` path:
///
/// - The Casper node binary;
/// - Client smart contracts;
/// - Chainspec template `chainspec.toml`
/// - Node config example `config.toml`.
#[derive(Debug, Clone)]
pub struct Artifacts(pub(crate) PathBuf);

/// Allows to build [`Artifacts`].
#[derive(Debug)]
pub struct ArtifactsBuilder {
    /// The place the code is located.
    location: Location,
    /// Tells if the binary will be (re)complied or not. The default depends on the location.
    pub compile: Option<bool>,
}

#[derive(Debug)]
enum Location {
    /// Local codebase on the disk.
    Local { project_path: Option<PathBuf> },
    /// We'll download the codebase.
    Remote {
        url: Option<String>,
        reference: TagOrHash,
    },
}

#[derive(Debug)]
enum TagOrHash {
    Tag(String),
    Hash(String),
}

impl Artifacts {
    /// Creates a builder for a new binary. By default, it tries and use the local code.
    pub fn builder() -> ArtifactsBuilder {
        ArtifactsBuilder {
            location: Location::Local { project_path: None },
            compile: None,
        }
    }

    /// Expert mode: at least the following must be present at the given path:
    ///
    /// - Node binary;
    /// - Client smart contracts.
    pub fn from_path<P: Into<PathBuf>>(path: P) -> Self {
        Self(path.into())
    }
}

impl ArtifactsBuilder {
    /// Builds the artifacts. The behavior is different for local code and remote
    /// one downloaded for the build.
    ///
    /// Local:
    /// - Compiles by default;
    /// - Not cached (artifacts are stored in the project directly);
    ///
    /// Remote:
    /// - Not compiled by default, in the sense that it tries and look in the cache first;
    /// - Cached in the default system location;
    pub async fn build(self) -> Result<Artifacts> {
        let Self { location, compile } = self;

        let artifacts = match location {
            Location::Local { project_path } => {
                let project_path = project_path
                    .unwrap_or_else(|| PathBuf::from("../casper-node"))
                    .canonicalize()
                    .map_err(Error::FailedToCanonicalizePath)?;
                let dest = project_path.join("target/").join(crate::PROJECT_DIR);

                if compile.unwrap_or(true) {
                    run_compilation(&project_path).await?;
                    // Let's copy everything to a canonical place:
                    copy_project_output_to(&project_path, &dest).await?;
                }

                Artifacts(dest)
            }
            Location::Remote { url, reference } => {
                let _ = (url, reference);
                //let url = url.as_deref().unwrap_or(NODE_GIT_URL);
                //let repo = match git2::Repository::clone(url, "/path/to/a/repo") {
                //    Ok(repo) => repo,
                //    Err(e) => panic!("failed to clone: {}", e),
                //};
                todo!("No remote download for now")
            }
        };

        Ok(artifacts)
    }

    /// Weither the project should be compiled or not. If the project is local, will attempt to
    pub fn compile(self, compile: bool) -> Self {
        Self {
            compile: Some(compile),
            ..self
        }
    }

    /// Specifies a local path to use the binary from.
    pub fn local_path(self, path: impl Into<PathBuf>) -> Self {
        Self {
            location: Location::Local {
                project_path: Some(path.into()),
            },
            ..self
        }
    }

    /// The binary will be downloaded from the official repository with the given hash.
    pub fn hash(self, hash: impl ToOwned<Owned = String>) -> Self {
        Self {
            location: Location::Remote {
                url: None,
                reference: TagOrHash::Hash(hash.to_owned()),
            },
            ..self
        }
    }

    /// The binary will be downloaded from the official repository with the given tag.
    pub fn tag(self, tag: impl ToOwned<Owned = String>) -> Self {
        Self {
            location: Location::Remote {
                url: None,
                reference: TagOrHash::Tag(tag.to_owned()),
            },
            ..self
        }
    }

    /// The binary will be downloaded from the given repository with the given hash.
    pub fn repo_hash(
        self,
        repo_url: impl ToOwned<Owned = String>,
        hash: impl ToOwned<Owned = String>,
    ) -> Self {
        Self {
            location: Location::Remote {
                url: Some(repo_url.to_owned()),
                reference: TagOrHash::Hash(hash.to_owned()),
            },
            ..self
        }
    }

    /// The binary will be downloaded from the given repository with the given tag.
    pub fn repo_tag(
        self,
        repo_url: impl ToOwned<Owned = String>,
        tag: impl ToOwned<Owned = String>,
    ) -> Self {
        Self {
            location: Location::Remote {
                url: Some(repo_url.to_owned()),
                reference: TagOrHash::Tag(tag.to_owned()),
            },
            ..self
        }
    }
}

/// Compiles the given project.
async fn run_compilation(path: &Path) -> Result<()> {
    //TODO use a logging crate
    println!("Path is {:?}", path);

    // Read the pinned versions. We'll use them later:
    let pinned_nightly = {
        let toolchain_path = path.join("smart_contracts/rust-toolchain");

        fs::read_to_string(&toolchain_path)
            .await
            .map_err(|io_err| Error::FileOperation {
                description: format!("reading the {toolchain_path:?}"),
                io_err,
            })?
            .lines()
            .next()
            .expect("No toolchain in the toolchain file")
            .to_owned()
    };
    let pinned_stable = {
        let toolchain_path = path.join("rust-toolchain.toml");

        let channel_line = fs::read_to_string(&toolchain_path)
            .await
            .map_err(|io_err| Error::FileOperation {
                description: format!("reading the {toolchain_path:?}"),
                io_err,
            })?
            .lines()
            .find(|&line| line.trim().starts_with("channel"))
            .expect("No channel specified for the pinned stable")
            .to_owned();

        let start = channel_line.find('"').expect("Channel to have version") + 1;
        let end = start
            + channel_line[start..]
                .find('"')
                .expect("Channel to have version");

        channel_line[start..end].to_owned()
    };
    log::debug!("Pinned Nightly: {pinned_nightly}");
    log::debug!("Pinned Stable: {pinned_stable}");

    // First, install the pinned toolchains, and the wasm target:
    let spinner = Spinner::create("Installing Rust components");

    spawn_process(
        path,
        [
            "rustup",
            "toolchain",
            "install",
            &pinned_stable,
            &pinned_nightly,
        ],
    )
    .await?
    .status_ok_or(ProcessError::FailedToSetupRust)?;
    spawn_process(
        path,
        [
            "rustup",
            "target",
            "add",
            "--toolchain",
            &pinned_stable,
            "wasm32-unknown-unknown",
        ],
    )
    .await?
    .status_ok_or(ProcessError::FailedToSetupRust)?;
    spawn_process(
        path,
        [
            "rustup",
            "target",
            "add",
            "--toolchain",
            &pinned_nightly,
            "wasm32-unknown-unknown",
        ],
    )
    .await?
    .status_ok_or(ProcessError::FailedToSetupRust)?;

    spinner.success();

    // Then, build the node binary:
    let spinner = Spinner::create("Building the node");

    spawn_process(
        path,
        [
            "cargo",
            &format!("+{pinned_stable}"),
            "build",
            "--release",
            "-p",
            "casper-node",
        ],
    )
    .await?
    .status_ok_or(ProcessError::FailedToBuildNode)?;

    spinner.success();

    // Then, build the client smart contracts:
    let spinner = Spinner::create("Building the smart contracts");
    let smart_contracts_path = path.join("smart_contracts/contracts/client");
    let params = {
        let mut dirs_reader = fs::read_dir(&smart_contracts_path)
            .await
            .map_err(|io_err| Error::FileOperation {
                description: format!(
                    "reading the smart contract directory {smart_contracts_path:?}"
                ),
                io_err,
            })?;
        let mut results = vec![
            OsString::from("cargo"),
            OsString::from(&format!("+{pinned_nightly}")),
            OsString::from("build"),
            OsString::from("--release"),
        ];

        while let Some(file_name) = dirs_reader
            .next_entry()
            .await
            .map_err(|io_err| Error::FileOperation {
                description: format!("reading the directory entry in {smart_contracts_path:?}"),
                io_err,
            })?
            .and_then(|entry| entry.path().file_stem().map(ToOwned::to_owned))
        {
            results.push(OsString::from("-p"));
            results.push(file_name);
        }

        results
    };

    spawn_process(path.join("smart_contracts/contracts"), params)
        .await?
        .status_ok_or(ProcessError::FailedToBuildSmartContracts)?;

    spinner.success();

    Ok(())
}

async fn copy_project_output_to(
    project_path: impl AsRef<Path>,
    dest: impl AsRef<Path>,
) -> Result<()> {
    let project_path = project_path.as_ref();
    let dest = dest.as_ref();
    let spinner = Spinner::create("Copying the files");

    // Create the destination:

    fs::create_dir_all(dest)
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("creating the destination directory {dest:?}"),
            io_err,
        })?;

    // Copy the wasm contracts:

    let contracts_source = project_path.join("target/wasm32-unknown-unknown/release/");
    let mut dir_reader =
        fs::read_dir(&contracts_source)
            .await
            .map_err(|io_err| Error::FileOperation {
                description: format!("reading the directory {contracts_source:?}"),
                io_err,
            })?;

    while let Some(file_path) = dir_reader
        .next_entry()
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("reading the directory entry in {contracts_source:?}"),
            io_err,
        })?
        .map(|entry| entry.path())
    {
        if file_path.extension() == Some(OsStr::new("wasm")) {
            fs::copy(&file_path, dest.join(file_path.file_name().unwrap()))
                .await
                .map_err(|io_err| Error::FileOperation {
                    description: format!("copying the file {:?} to {dest:?}", file_path),
                    io_err,
                })?;
        }
    }

    // Copy the node binary:

    let node_path = project_path.join("target/release/casper-node");

    fs::copy(&node_path, &dest.join(node_path.file_name().unwrap()))
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("copying the node {:?} to {dest:?}", node_path),
            io_err,
        })?;

    // Copy the local config example:

    let config_path = project_path.join("resources/local/config.toml");

    fs::copy(&config_path, &dest.join(config_path.file_name().unwrap()))
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("copying the config {:?} to {dest:?}", config_path),
            io_err,
        })?;

    // Copy the local chainspec template:

    let chainspec_path = project_path.join("resources/local/chainspec.toml.in");

    fs::copy(&chainspec_path, &dest.join("chainspec.toml"))
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("copying the chainspec {:?} to {dest:?}", chainspec_path),
            io_err,
        })?;

    Ok(spinner.success())
}
