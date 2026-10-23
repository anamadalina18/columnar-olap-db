use crate::iceberg::{Catalog, ColumnStats, FileStats, Manifest, MinMax};
use crate::storage::{DataFile, DataFileHeader, FileBasedStorage, FileHandle};
use crate::{DatabaseError, Record, TableChunk, TypeID, Value};
use std::collections::HashSet;

use crate::engine::SdmsIcebergEngine;

impl SdmsIcebergEngine {
    pub fn new(catalog: Catalog, storage: FileBasedStorage) -> SdmsIcebergEngine {
        SdmsIcebergEngine {
            catalog,
            manifest: Manifest::default(),
            table_id: None,
            storage,
            changed_files: HashSet::new(),
        }
    }

    /// Starts a new table modification by setting the table_id member.
    /// Needs to be called before "insert/update/delete/delete_chunks".
    /// A table modification is concluded by calling "commit".
    pub fn start_table_modification(&mut self, table_id: usize) -> Result<(), DatabaseError> {
        if self.table_id.is_some() || !self.catalog.check_table_exists(table_id) {
            return Err(DatabaseError::EngineError);
        }
        self.table_id = Some(table_id);
        Ok(())
    }

    /// End a modification, creating a new version if files have changed.
    /// Stops the current modification by resetting the table_id.
    pub fn commit(&mut self) -> Result<(), DatabaseError> {
        if let Some(table_id) = self.table_id {
            let table_metadata = self.catalog.get_table_metadata(table_id);

            if !self.changed_files.is_empty() {
                // Moves self.manifest to table_metadata and replaces it with Manifest::default()
                table_metadata.add_version(std::mem::take(&mut self.manifest));
                self.changed_files = HashSet::new();
            }
            self.table_id = None;

            Ok(())
        } else {
            Err(DatabaseError::EngineError)
        }
    }

    /// We suggest to implement this helper function to calculate MinMax FileStats for a chunk.
    fn calculate_statistics(chunk: &TableChunk) -> FileStats {
        let mut vec:Vec<ColumnStats> = Vec::new();

        for column in chunk {
            let mut min = column.get(0).unwrap().clone();
            let mut max = min.clone();

            for elem in column {
                if *elem < min {
                    min = elem.clone();
                }

                if *elem > max {
                    max = elem.clone();
                }
            }

           vec.push(ColumnStats::MinMax(MinMax::new(min, max)));
        }

        FileStats::new(vec)
    }

    /// Insert the chunks into the table by calculating their stats, writing them to a file each
    /// using the storage module, and adding them to the manifest.
    /// Return DatabaseError::EngineError if no table modification is ongoing.
    pub fn insert(&mut self, chunks: Vec<TableChunk>) -> Result<(), DatabaseError> {
        if self.table_id == None {
            return Err(DatabaseError::EngineError);
        }

        for chunk in chunks {
            let stats = Self::calculate_statistics(&chunk);
            let mut column_info: Vec<TypeID> = Vec::new();

            for col in chunk.clone() {
                let val = col[0].get_id();
                column_info.push(val.clone());
            }

            let data_file_header:DataFileHeader = DataFileHeader::new(chunk[0].len() as u64, chunk.len() as u64, column_info);
            let file:DataFile = DataFile::new(data_file_header, chunk);

            let file_handle = self.storage.write_file(file.to_bytes().as_slice())?;

            self.manifest.add_file(file_handle, stats);
            self.changed_files.insert(file_handle);
        }


        Ok(())
    }

    /// Update the table by loading the changed chunks from their files, overwriting the rows with
    /// the records given in updates, writing the changed chunk into a new file, calculating new
    /// statistics, and recording the changes in the manifest.
    /// If a file handle is already contained in the changes of this manifest, do not change files
    /// or the manifest and return DatabaseError::EngineError.
    /// Return DatabaseError::EngineError if no table modification is ongoing.
    ///
    /// An update consists of a row ID (u32) and a Record containing the new data for this row.
    /// Multiple rows in one file can be updated by one call of this function (Vec<(u32, Record)>).
    /// Multiple files can be updated by one call of this function by calling it with a vector of
    /// FileHandles and corresponding updates
    pub fn update(
        &mut self,
        updates: Vec<(FileHandle, Vec<(u32, Record)>)>,
    ) -> Result<(), DatabaseError> {
        if self.table_id == None {
            return Err(DatabaseError::EngineError);
        }

        let table_metadata = self.catalog.get_table_metadata(self.table_id.unwrap());

        let vec:Vec<FileHandle> = updates.iter().map(|(file, vec)| file.clone()).collect();
        if have_duplicates(vec) {
            return Err(DatabaseError::EngineError);
        }

        for (file_handle, update_vec) in updates.clone() {
            if self.changed_files.contains(&file_handle) {
                return Err(DatabaseError::EngineError);
            }
            if !table_metadata.files(None).contains(&file_handle) {
                return Err(DatabaseError::EngineError);
            }
        }

        for (file_handle, update_vec) in updates.clone() {
            let data_file = DataFile::parse(&mut self.storage.read_file(&file_handle)?)?;
            let chunk = data_file.data;
            let mut new_chunk = chunk.clone();

            for (idx, rec) in update_vec {
                for (i, col) in new_chunk.iter_mut().enumerate() {
                    col[idx as usize] = rec.record[i].clone();
                }
            }

            self.insert(vec![new_chunk])?;
            self.manifest.delete_file(file_handle);
            self.changed_files.insert(file_handle);
        }

        Ok(())
    }

    /// Update the table by loading the specified chunks from their files, deleting the rows
    /// specified in deletions, storing the changed chunks in new files, calculating new
    /// statistics, and recording the changes in the manifest.
    /// You can assume that the row indexes in deletions are in ascending order!
    /// If a file handle is already contained in the changes of this manifest, do not change files
    /// or the manifest and return DatabaseError::EngineError.
    /// Return DatabaseError::EngineError if no table modification is ongoing.
    ///
    /// A row to delete is represented as u32.
    /// Multiple rows in one file can be deleted by one call of this function (Vec<u32>).
    /// Rows in Multiple files can be deleted by one call of this function by calling it with a
    /// vector of FileHandles and corresponding deletions.
    pub fn delete(&mut self, deletions: Vec<(FileHandle, Vec<u32>)>) -> Result<(), DatabaseError> {
        if self.table_id == None {
            return Err(DatabaseError::EngineError);
        }

        let table_metadata = self.catalog.get_table_metadata(self.table_id.unwrap());

        let vec:Vec<FileHandle> = deletions.iter().map(|(file, vec)| file.clone()).collect();
        if have_duplicates(vec) {
            return Err(DatabaseError::EngineError);
        }

        for (deletion, vec) in deletions.clone() {
            if self.changed_files.contains(&deletion) {
                return Err(DatabaseError::EngineError);
            }
            if !table_metadata.files(None).contains(&deletion) {
                return Err(DatabaseError::EngineError);
            }
        }

        for (file_handle, delete_vec) in deletions.clone() {
            let data_file = DataFile::parse(&mut self.storage.read_file(&file_handle)?)?;
            let chunk = data_file.data;

            if delete_vec.len() == chunk[0].len() {
                self.delete_chunks(vec![file_handle].as_slice());
                continue;
            }

            let mut new_chunk = chunk.clone();

            for idx in delete_vec.iter().rev() {
                for col in new_chunk.iter_mut() {
                    col.remove(*idx as usize);
                }
            }

            self.insert(vec![new_chunk])?;
            self.manifest.delete_file(file_handle);
            self.changed_files.insert(file_handle);
        }


        Ok(())
    }

    /// Update the table by marking the FileHandles in deletions as deleted in the manifest.
    /// If a file handle is already contained in the changes of this manifest, do not change files
    /// or the manifest and return DatabaseError::EngineError.
    /// Return DatabaseError::EngineError if no table modification is ongoing.
    pub fn delete_chunks(&mut self, deletions: &[FileHandle]) -> Result<(), DatabaseError> {
        if self.table_id == None {
            return Err(DatabaseError::EngineError);
        }

        let table_metadata = self.catalog.get_table_metadata(self.table_id.unwrap());

        for deletion in deletions {
            if self.changed_files.contains(deletion) {
                return Err(DatabaseError::EngineError);
            }
            if !table_metadata.files(None).contains(deletion) {
                return Err(DatabaseError::EngineError);
            }
        }

        for deletion in deletions {
            self.manifest.delete_file(*deletion);
            self.changed_files.insert(*deletion);
        }
        Ok(())
    }
}

pub fn have_duplicates(deletions: Vec<FileHandle>) -> bool {
    let hash_set:HashSet<FileHandle> = HashSet::from_iter(deletions.iter().cloned());

    return hash_set.len() != deletions.len();
}
