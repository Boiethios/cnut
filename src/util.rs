mod spinner;
pub use spinner::Spinner;
mod dir;
pub use dir::cache;
pub mod crypto;
mod process;
pub use process::NodeProcess;

use crate::error::{ProcessError, Result};
use std::{
    ffi::OsStr,
    path::Path,
    process::Output,
    sync::{atomic::AtomicU8, Arc},
};
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

pub(crate) fn create_update_table(toml: &mut toml::Value, args: &[&str], value: toml::Value) {
    match args {
        [] => *toml = value,
        [arg, rest @ ..] => {
            if let toml::Value::Table(table) = toml {
                let toml = match table.entry(arg.to_owned()) {
                    toml::map::Entry::Vacant(vacant) => vacant.insert(toml::Table::new().into()),
                    toml::map::Entry::Occupied(occupied) => occupied.into_mut(),
                };

                create_update_table(toml, rest, value)
            } else {
                panic!("{arg:?} is not the key for a table")
            }
        }
    }
}

/// Creates a TOML map.
macro_rules! toml_map {
    () => { toml::Table::new() };
    ( $( $first_keys:expr ),+ => $first_value:expr $( , $( $keys:expr ),+ => $value:expr )* $(,)? ) => {{
        let mut map = toml::Table::new().into();
        // There is no reason to add twice the same key. In such a case, this
        // is treated as a bug.
        crate::util::create_update_table(&mut map, [$(
            $first_keys,
        )+].as_ref(), toml::Value::from($first_value));
        $(
            crate::util::create_update_table(&mut map, [$(
                $keys,
            )+].as_ref(), toml::Value::from($value));
        )+

        match map {
            toml::Value::Table(table) => table,
            _ => unreachable!("map is a table"),
        }
    }};
}
pub(crate) use toml_map;

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
pub fn update_toml(mut content: toml::Value, updates: toml::Table) -> toml::Value {
    pub fn merge_toml_value(base: &mut toml::Value, other: toml::Value) {
        match (base, other) {
            (toml::Value::Table(base_table), toml::Value::Table(other_table)) => {
                // Update the base table, by joining both, merging in the values from `other_value` on conflict.
                for (key, value) in other_table {
                    match base_table.entry(key) {
                        toml::map::Entry::Vacant(vacant) => {
                            vacant.insert(value);
                        }
                        toml::map::Entry::Occupied(occupied) => {
                            let old = occupied.into_mut();
                            merge_toml_value(old, value);
                        }
                    }
                }
            }
            // Any other value just results in a replacement.
            (base, other) => *base = other,
        }
    }

    merge_toml_value(&mut content, updates.into());

    content
}

/// 0: network is running.
/// 1: network must shut down.
/// 2: network has already shut down.
#[derive(Debug, Default, Clone)]
pub struct ShutdownState(Arc<AtomicU8>);

impl Drop for ShutdownState {
    fn drop(&mut self) {
        // If it's the last reference, mark the node as shutting down:
        if Arc::strong_count(&self.0) == 1 {
            self.set_shut_down();
        }
    }
}

impl ShutdownState {
    pub fn must_shut_down(&self) -> bool {
        let order = std::sync::atomic::Ordering::Relaxed;
        match self.0.compare_exchange(1, 2, order, order) {
            Ok(_must_shut_down) => true,
            Err(_running_or_has_shut_down) => false,
        }
    }

    pub fn set_shut_down(&self) {
        // If it's on 0:running, store that it must shut down:
        let order = std::sync::atomic::Ordering::Relaxed;
        let _ = self.0.compare_exchange(0, 1, order, order);
    }
}
