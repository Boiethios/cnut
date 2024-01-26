use crate::{
    error::{Error, Result},
    PROJECT_NAME,
};
use std::path::PathBuf;

/// Returns the directory where the binaries cache is.
pub fn cache() -> Result<PathBuf> {
    Ok(canonical_user_dirs()?.cache_dir().to_owned())
}

fn canonical_user_dirs() -> Result<directories::ProjectDirs> {
    directories::ProjectDirs::from("network", "Casper", PROJECT_NAME)
        .ok_or(Error::FailedToFindBaseDirectory)
}
