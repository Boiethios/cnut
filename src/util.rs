mod spinner;
pub use spinner::Spinner;
mod dir;
pub use dir::cache;
pub mod crypto;

use crate::error::{Error, ProcessError, Result};
use std::{collections::HashMap, ffi::OsStr, path::Path, process::Output};
use tokio::process::Command;

pub trait ProcessOutputExt {
    fn status_ok_or(self, error: fn(Output) -> ProcessError) -> Result<()>;
}

impl ProcessOutputExt for Output {
    fn status_ok_or(self, error: fn(Output) -> ProcessError) -> Result<()> {
        if self.status.success() {
            Ok(())
        } else {
            Err(error(self).into())
        }
    }
}

macro_rules! map {
    () => { std::collections::HashMap::new() };
    ( $first_key:expr => $first_value:expr $( , $key:expr => $value:expr )* $(,)? ) => {{
        let mut map = std::collections::HashMap::new();
        // There is no reason to add twice the same key.
        // Since it's used for testing, we can panic in such a case:
        assert!(map.insert($first_key, $first_value).is_none());
        $(
            assert!(map.insert($key, $value).is_none());
        )*
        map
    }};
}
pub(crate) use map;

pub async fn spawn_process<S: AsRef<OsStr>>(
    path: impl AsRef<Path>,
    params: impl AsRef<[S]>,
) -> Result<Output> {
    let params = params.as_ref();
    let full_command = params
        .iter()
        .map(|s| s.as_ref().to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");

    log::debug!("Spawning Command: {full_command}");

    Command::new(params[0].as_ref())
        .args(&params[1..])
        .current_dir(path)
        .output()
        .await
        .map_err(|io_err| crate::error::Error::FailedToSpawnProcess {
            full_command,
            io_err,
        })
}

pub struct LettersGen(Vec<u8>);

impl LettersGen {
    pub fn new() -> Self {
        LettersGen(vec![b'A'])
    }

    pub fn next(&mut self) -> String {
        let result: String = String::from_utf8_lossy(&self.0).to_string();
        self.increment_letters();
        result
    }

    fn increment_letters(&mut self) {
        let mut carry = true;
        for letter in self.0.iter_mut().rev() {
            if carry {
                *letter = match *letter {
                    b'Z' => b'A',
                    _ => {
                        carry = false;
                        *letter + 1
                    }
                };
            }
        }
        if carry {
            self.0.insert(0, b'A');
        }
    }
}

/// Update the given paths of a TOML file with the given value.
pub fn update_toml(
    mut content: toml::Value,
    updates: HashMap<&[&str], toml::Value>,
) -> Result<toml::Value> {
    for (keys, value) in updates {
        let leaf = keys.iter().try_fold(&mut content, |node, &key| {
            // If the key exist, just return the value at the key.
            if node.get(key).is_some() {
                Ok(node.get_mut(key).unwrap())
            // Otherwise, try to insert it:
            } else if let Some(slot) = node.as_table_mut() {
                slot.insert(key.to_owned(), toml::map::Map::new().into());
                Ok(node.get_mut(key).unwrap())
            // If we cannot insert it, return an error: not a table:
            } else {
                Err(Error::TomlEdit {
                    value: node.clone(),
                })
            }
        })?;
        *leaf = value;
    }

    Ok(content)
}
