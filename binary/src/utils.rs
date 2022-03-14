use std::{
    io::prelude::*,
    fs::{read_to_string, remove_file, create_dir_all, File},
    path::{Path, PathBuf}
};

use anyhow::{anyhow, Context, Result as AnyHow};
use serde::{Deserialize, Serialize};
use slice_diff_patch::Change;
use regex::Regex;

use crate::struct_diff::*;

pub const IO_SEPARATOR: &str = "OMFG_IO_SEPARATOR";

pub trait DebugPrint {
    fn dprint(self, ident: &str) -> Self;
}

impl<T> DebugPrint for T
where
    T: std::fmt::Debug + Clone + Send + Sync
{
    fn dprint(self, ident: &str) -> Self {
        println!("{} {:?}", ident, self);
        self
    }
}

pub trait PathBufExt {
    fn read(&self) -> AnyHow<String>;
    fn write_plus(&self, content: &str) -> AnyHow<()>;
    fn remove(&self) -> AnyHow<()>;
    fn copy_from(&self, other: &Path) -> AnyHow<u64>;
}

impl PathBufExt for PathBuf {
    fn read(&self) -> AnyHow<String> {
        read_to_string(self).map_err(|e| anyhow!("Failed to read file: {}", e))
    }

    fn write_plus(&self, content: &str) -> AnyHow<()> {
        create_dir_all(self.parent().context("Invalid path: {}")?)?;
        File::create(self)?
            .write_all(content.as_bytes())
            .map_err(|e| anyhow!("Failed to write file: {}", e))
    }

    fn remove(&self) -> AnyHow<()> {
        remove_file(self).map_err(|e| anyhow!("Failed to remove file: {}", e))
    }

    fn copy_from(&self, other: &Path) -> AnyHow<u64> {
        let parent = self.parent().context("Invalid path: {}")?;
        create_dir_all(parent)?;
        std::fs::copy(other, self).map_err(|e| anyhow!("Failed to copy file: {}", e))
    }
}

//  Language limitation: Orphan rules.
//  Not simple structs so can't use serde remote.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DividerDef {
    Delimited {
        prefix: String,
        open: String,
        close: String,
    },
    Headings {
        fuzzed: String,
        strict: Option<String>,
        indent: String,
    },
    Enclosures {
        top: String,
        bottom: String,
    },
}

impl DividerDef {
    pub fn try_into_divider(self) -> Option<Divider> {
        match self {
            Self::Delimited { prefix, open, close } => {
                match (Regex::new(&prefix), Regex::new(&open), Regex::new(&close)) {
                    (Ok(prefix), Ok(open), Ok(close)) => Some(
                        Divider::Delimited {
                            prefix,
                            open,
                            close
                        }
                    ),
                    _ => None
                }
            },
            Self::Headings { fuzzed, strict, indent } => {
                match (Regex::new(&fuzzed), strict.and_then(|s| Regex::new(&s).ok())) {
                    (Ok(fuzzed), strict) => Some(
                        Divider::Headings {
                            fuzzed,
                            strict,
                            indent
                        }
                    ),
                    _ => None
                }
            },
            Self::Enclosures { top, bottom } => {
                match (Regex::new(&top), Regex::new(&bottom)) {
                    (Ok(top), Ok(bottom)) => Some(
                        Divider::Enclosures {
                            top,
                            bottom
                        }
                    ),
                    _ => None
                }
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum ChangeString {
    Remove(usize),
    Insert((usize, String)),
    Update((usize, String)),
}

impl From<ChangeString> for Change<String> {
    fn from(change: ChangeString) -> Self {
        match change {
            ChangeString::Remove(i) => Change::Remove(i),
            ChangeString::Insert((i, s)) => Change::Insert((i, s)),
            ChangeString::Update((i, s)) => Change::Update((i, s)),
        }
    }
}

impl From<Change<String>> for ChangeString {
    fn from(change: Change<String>) -> Self {
        match change {
            Change::Remove(i) => ChangeString::Remove(i),
            Change::Insert((i, s)) => ChangeString::Insert((i, s)),
            Change::Update((i, s)) => ChangeString::Update((i, s)),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct StructDiffDef {
    comment: String,
    changes: Vec<ChangeString>,
    removed: Vec<usize>,
    added: Vec<usize>
}

impl From<StructDiffDef> for StructDiff {
    fn from(mod_def: StructDiffDef) -> Self {
        StructDiff {
            comment: mod_def.comment,
            removed: mod_def.removed,
            added: mod_def.added,
            changes: mod_def.changes.into_iter().map(|c| c.into()).collect()
        }
    }
}

impl From<StructDiff> for StructDiffDef {
    fn from(mod_def: StructDiff) -> Self {
        StructDiffDef {
            comment: mod_def.comment,
            removed: mod_def.removed,
            added: mod_def.added,
            changes: mod_def.changes.into_iter().map(|c| c.into()).collect()
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct KeyDef {
    fuzzed: Option<String>,
    strict: String,
}

impl KeyDef {
    fn try_into_key(self) -> Option<Key> {
        match (self.fuzzed.map(|s| Regex::new(&s)).transpose(), Regex::new(&self.strict)) {
            (Ok(fuzzed), Ok(strict)) => Some(Key {
                fuzzed,
                strict
            }),
            _ => None
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ConfigDef {
    filter: Option<DividerDef>,
    expander: Option<DividerDef>,
    keys: Vec<KeyDef>,
}

impl From<ConfigDef> for Config {
    fn from(config_def: ConfigDef) -> Self {
        let filter = config_def.filter.map(
            |divider| divider
                .try_into_divider()
                .expect("Invalid filter")
        );

        let expander = config_def.expander.map(
            |divider| divider
                .try_into_divider()
                .expect("Invalid expander")
        );

        let keys = config_def.keys.into_iter().map(
            |keydef| keydef
                .try_into_key()
                .expect("Invalid key")
        );
        
        Self {
            filter,
            expander,
            keys: keys.collect()
        }
    }
}
