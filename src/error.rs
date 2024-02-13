//! Holds all the error-related code.

use std::{fmt, io::Error as IoError, process::Output as ProcessOutput};
use thiserror::Error;

/// Main result type for this library.
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for this library.
#[derive(Error)]
pub enum Error {
    /// The user's base directory could not be found, for example
    /// `~/.local/casper` for Linux systems.
    #[error("failed to find the user's base directory for this system")]
    FailedToFindBaseDirectory,

    /// The path `canonicalize`` function failed, meaning that a provided
    /// path was somehow invalid.
    #[error("failed canonicalize the path: {0}")]
    FailedToCanonicalizePath(IoError),

    /// A child process could not be spawned.
    #[error("failed to spawn the process `{full_command}` because {io_err}")]
    FailedToSpawnProcess {
        /// The command run.
        full_command: String,
        /// The underlying IO error causing the process not to spawn.
        #[source]
        io_err: IoError,
    },

    /// A child process returned with an error code.
    #[error(transparent)]
    ChildProcess(#[from] ProcessError),

    /// A file operation failed. See the `description` field to know what exact
    /// operation failed.
    #[error("failed doing the file operation: `{description}` because {io_err}")]
    FileOperation {
        /// The command run.
        description: String,
        /// The underlying IO error causing the process not to spawn.
        #[source]
        io_err: IoError,
    },

    /// A read TOML file was invalid.
    #[error(transparent)]
    TomlParsing(#[from] toml::de::Error),

    /// Encoding the DER info failed.
    #[error(transparent)]
    DerEncoding(#[from] derp::Error),

    /// There were an error while creating an ed25519 private key.
    #[error("{:?}", .0)]
    Ed25519(ed25519_dalek::pkcs8::spki::der::pem::Error),

    /// There were an error while starting the web server.
    #[error("Failed to start the web server: {:?}", .0)]
    StartingServerWeb(IoError),

    /// There is no node with this name.
    #[error("Node does not exist: {}", .0)]
    NodeNameNotFound(String),

    /// There is no node with this index.
    #[error("Node does not exist: {}", .0)]
    NodeIndexOutOfBounds(usize),
}

/// Error used to show the error a child process returned.
#[derive(Error)]
pub enum ProcessError {
    /// The Rust tools needed to compile the node could not be installed
    /// successfully.
    #[error("failed to install the Rust tools, exited with code {}", .0.status)]
    FailedToSetupRust(ProcessOutput),
    /// The Casper node binary failed to compile.
    #[error("failed to build the Casper node, exited with code {}", .0.status)]
    FailedToBuildNode(ProcessOutput),
    /// The Casper client smart contracts failed to compile.
    #[error("failed to build the client smart contracts, exited with code {}", .0.status)]
    FailedToBuildSmartContracts(ProcessOutput),
}

impl From<ed25519_dalek::pkcs8::spki::der::pem::Error> for Error {
    fn from(value: ed25519_dalek::pkcs8::spki::der::pem::Error) -> Self {
        Self::Ed25519(value)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FailedToFindBaseDirectory => write!(f, "FailedToFindBaseDirectory"),
            Self::FailedToCanonicalizePath(arg0) => f
                .debug_tuple("FailedToCanonicalizePath")
                .field(arg0)
                .finish(),
            Self::FailedToSpawnProcess {
                full_command,
                io_err,
            } => f
                .debug_struct("FailedToSpawnProcess")
                .field("full_command", full_command)
                .field("io_err", io_err)
                .finish(),
            Self::ChildProcess(arg0) => write!(f, "ChildProcess: {arg0:?}"), //f.debug_tuple("ChildProcess").field(arg0).finish(),
            Self::FileOperation {
                description,
                io_err,
            } => f
                .debug_struct("FileOperation")
                .field("description", description)
                .field("io_err", io_err)
                .finish(),
            Self::TomlParsing(e) => write!(f, "TomlParsing({e:?})"),
            Self::DerEncoding(e) => write!(f, "DerEncoding({e:?})"),
            Self::Ed25519(e) => write!(f, "Ed25519({e:?})"),
            Self::StartingServerWeb(e) => write!(f, "StartingServerWeb({e:?})"),
            Self::NodeNameNotFound(name) => write!(f, "NodeNameNotFound({name})"),
            Self::NodeIndexOutOfBounds(index) => write!(f, "NodeIndexOutOfBounds({index})"),
        }
    }
}

impl fmt::Debug for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FailedToSetupRust(ProcessOutput {
                status,
                stdout: _,
                stderr,
            }) => write!(
                f,
                "FailedToSetupRust:\n\tStatus: {status:?}\n\tOutput:\n{}",
                String::from_utf8_lossy(stderr)
            ),

            Self::FailedToBuildNode(ProcessOutput {
                status,
                stdout: _,
                stderr,
            }) => write!(
                f,
                "FailedToBuildNode:\n\tStatus: {status:?}\n\tOutput:\n{}",
                String::from_utf8_lossy(stderr)
            ),

            Self::FailedToBuildSmartContracts(ProcessOutput {
                status,
                stdout: _,
                stderr,
            }) => write!(
                f,
                "FailedToBuildSmartContracts:\n\tStatus: {status:?}\n\tOutput:\n{}",
                String::from_utf8_lossy(stderr)
            ),
        }
    }
}
