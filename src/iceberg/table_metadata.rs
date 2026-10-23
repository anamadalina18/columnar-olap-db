use std::cmp::{max, min};
use super::{Manifest, TableMetadata, version::Version};
use crate::storage::FileHandle;
use crate::{Schema, Value};
use std::collections::HashSet;
use std::ops::RangeInclusive;

impl TableMetadata {
    pub fn new(name: String, schema: Schema) -> Self {
        TableMetadata {
            name,
            schema,
            version: Version::default(),
            manifests: Vec::new(),
        }
    }

    /// Return all manifests (versions) until the input version as a slice. If version is None,
    /// return all manifests (the current version).
    pub fn snapshot(&self, version: Option<Version>) -> &[Manifest] {
        match version {
            None => {self.manifests.as_slice()},
            Some(v) => {
                let inner = u64::from(v);
                self.manifests[0..min(inner, self.manifests.len() as u64) as usize].iter().as_slice()
            }
        }
    }

    /// Return all file handles of files that belong to the table at the specified version.
    pub fn files(&self, version: Option<Version>) -> HashSet<FileHandle> {
         let manifests = self.snapshot(version);
         let mut hash_set:HashSet<FileHandle> = HashSet::new();

         for manifest in manifests.iter() {
             hash_set.extend(manifest.added.iter());
             hash_set.retain(|file| {
                 !manifest.deleted.contains(file)
             });
         }

         hash_set
    }

    /// Return all file handles of files that belong to the table at the specified version and
    /// overlap with all predicates. Predicates are represented as the value to search for and the
    /// column ID as usize. Predicates are conjunctive. Thus, all predicates must match.
    pub fn contains(
        &self,
        predicates: Vec<(Value, usize)>,
        version: Option<Version>,
    ) -> HashSet<FileHandle> {
        let manifests = self.snapshot(version);
        let mut hash_set:HashSet<FileHandle> = HashSet::new();

        for manifest in manifests.iter() {
            hash_set.extend(manifest.contains(predicates.clone()));
            hash_set.retain(|file| {
                !manifest.deleted.contains(file)
            });
        }

        hash_set
    }

    /// Return all file handles of files that belong to the table at the specified version and
    /// overlap with all predicates. Predicates are represented as the range to search for and the
    /// column ID. Predicates are conjunctive. Thus, all predicates must match.
    pub fn contains_range(
        &self,
        predicates: Vec<(RangeInclusive<Value>, usize)>,
        version: Option<Version>,
    ) -> HashSet<FileHandle> {
        let manifests = self.snapshot(version);
        let mut hash_set:HashSet<FileHandle> = HashSet::new();

        for manifest in manifests.iter() {
            hash_set.extend(manifest.contains_range(predicates.clone()));
            hash_set.retain(|file| {
                !manifest.deleted.contains(file)
            });
        }

        hash_set
    }

    /// Add a manifest as a new version to the table and update the table version
    pub fn add_version(&mut self, manifest: Manifest) -> Version {
        self.manifests.push(manifest);
        self.version = self.version.successor();
        self.version
    }
}
