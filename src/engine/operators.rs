use crate::{Operator, Record};
use crate::storage::{Columns, DataFile};
use crate::storage::{FileBasedStorage, FileHandle};
use crate::{TableChunk, Value};
use std::collections::{HashMap, HashSet};

pub struct ColumnTableScan {
    // Add the necessary members here
    files: Vec<FileHandle>,
    col_project: Columns,
    storage: FileBasedStorage,
    position: usize,
    open_bool: bool,
}

impl ColumnTableScan {
    pub fn new(files: Vec<FileHandle>, col_project: Columns, storage: FileBasedStorage) -> Self {
        ColumnTableScan {
            files,
            col_project,
            storage,
            position: 0,
            open_bool: false
        }
    }
}

impl Operator for ColumnTableScan {
    /// Initialise child operators and itself.
    fn open(&mut self) {
        self.open_bool = true;
        self.position = 0;
    }

    /// Get the next chunk (part of the table), in this case the columns of one file.
    /// Returns [None] if there are no more files to read.
    /// Ensure that the TableChunk.len() is always the total number of columns in the table.
    fn next(&mut self) -> Option<TableChunk> {
        if self.open_bool == false {
            panic!("not opened");
        }
        if self.files.len() == self.position {
            return None;
        }
        let file = self.files.get(self.position)?;
        let data_file = DataFile::parse_columns(&mut self.storage.read_file(&file).unwrap(),
                                                self.col_project.clone()).unwrap();
        self.position += 1;

        Some(data_file.data)
    }
    // Cleans up child and itself.
    fn close(&mut self) {
        self.open_bool = false;
    }
}

/// An operator for conjunctive filters.
pub struct ColumnFilter {
    // Add the necessary members here
    child: Box<dyn Operator>,
    filters: HashMap<usize, (Value,Value)>,
    open_bool: bool
}

impl ColumnFilter {
    pub fn new(child: Box<dyn Operator>, filters: HashMap<usize, (Value, Value)>) -> Self {
        ColumnFilter {
            child,
            filters,
            open_bool: false
        }
    }
}

impl Operator for ColumnFilter {
    // Initialise child operators and itself.
    fn open(&mut self) {
        self.open_bool = true;
        self.child.open();
    }

    /// Get the next chunk (part of the table), filtered down to respective rows.
    /// Returns None if there are no more chunks to read.
    /// Preserve the order of rows per chunk.
    fn next(&mut self) -> Option<TableChunk> {
        if self.open_bool == false {
            panic!("not opened");
        }
        let op = self.child.next();
        if op.is_none() {
            return None;
        }

        let mut delete_rows:HashSet<usize> = HashSet::new();
        let mut chunk = op.unwrap();

        for (idx, (v1, v2)) in self.filters.clone() {
            let col = chunk[idx].clone();

            for (i, r) in col.iter().enumerate() {
                if *r < v1 || *r > v2 {
                    delete_rows.insert(i);
                }
            }
        }

        let mut vec:Vec<usize> = delete_rows.into_iter().collect();
        vec.sort();

        for idx in vec.iter().rev() {
            for col in chunk.iter_mut() {
                if col.len() > *idx {
                    col.remove(*idx);
                }
            }
        }

        Some(chunk)
    }

    /// Cleans up child and itself.
    fn close(&mut self) {
        self.open_bool = false;
        self.child.close();
    }
}

pub type AggFunc = Box<dyn Fn(&Vec<Value>) -> Value>;

pub struct ColumnAggregate {
    // Add the necessary members here
    child: Box<dyn Operator>,
    aggregates: HashMap<usize, AggFunc>,
    open_bool: bool
}

impl ColumnAggregate {
    pub fn new(child: Box<dyn Operator>, aggregates: HashMap<usize, AggFunc>) -> Self {
        ColumnAggregate {
            child,
            aggregates,
            open_bool: false
        }
    }
}

impl Operator for ColumnAggregate {
    /// Initialise child operators and itself.
    fn open(&mut self) {
        self.child.open();
        self.open_bool = true;
    }

    /// Returns a chunk containing the projected and aggregated column values.
    /// Returns exactly one row.
    /// Preserve the order of aggregation columns.
    fn next(&mut self) -> Option<TableChunk> {
        if self.open_bool == false {
            panic!("not opened");
        }
        let mut op = self.child.next();
        if op == None {
            return None;
        }

        let mut chunk = op.unwrap();
        op = self.child.next();
        while op != None {
            let mut next_chunk = op.unwrap();

            for (i, col) in chunk.iter_mut().enumerate() {
                let mut col0 = next_chunk[i].clone();
                col.append(&mut col0);
            }

            op = self.child.next();
        }

        let mut vec_idx:Vec<usize> = Vec::new();

        for (idx, func) in self.aggregates.iter_mut() {
            chunk[*idx] = vec![func(&chunk[*idx])];
            vec_idx.push(*idx);
        }

        for i in (0..chunk.len()).rev() {
            if !vec_idx.contains(&i) {
                chunk.remove(i);
            }
        }

        for i in (0..chunk.len()).rev() {
            if chunk[i].is_empty() {
               chunk.remove(i);
            }
        }

        Some(chunk)
    }

    /// Cleans up child and itself.
    fn close(&mut self) {
        self.open_bool = false;
        self.child.close();
    }
}

pub struct ColumnEqJoin {
    // Add the necessary members here
    child0: Box<dyn Operator>,
    child1: Box<dyn Operator>,
    join_column_idxs: (usize, usize),
    open_bool: bool,
    hash_map: HashMap<Value, Vec<Option<Value>>>,
    left_columns: usize
}

impl ColumnEqJoin {
    pub fn new(
        child0: Box<dyn Operator>,
        child1: Box<dyn Operator>,
        join_column_idxs: (usize, usize),
    ) -> Self {
        ColumnEqJoin {
            child0,
            child1,
            join_column_idxs,
            open_bool: false,
            hash_map: HashMap::new(),
            left_columns: 0
        }
    }
}

impl Operator for ColumnEqJoin {
    /// Initialize child operators and itself.
    fn open(&mut self) {
        self.child0.open();
        self.child1.open();
        self.open_bool = true;

        let mut op = self.child0.next();
        let mut chunk0:TableChunk = TableChunk::new();
        while op != None {
            let mut chunk = op.unwrap();

            if chunk0 == TableChunk::new() {
                chunk0 = chunk.clone();
                op = self.child0.next();
                continue;
            }

            for (col, col0) in chunk.iter_mut().zip(chunk0.iter_mut()) {
                col0.append(col);
            }

            op = self.child0.next();
        }

        self.left_columns = chunk0.len();

        for row in 0..chunk0[self.join_column_idxs.0].len() {
            let mut vec:Vec<Option<Value>> = Vec::new();
            for col in chunk0.iter() {
                if col.is_empty() {
                    vec.push(None);
                } else {
                    vec.push(Some(col[row].clone()));
                }
            }
            self.hash_map.insert(chunk0[self.join_column_idxs.0][row].clone(), vec);
        }
    }

    /// Get the next chunk of child0 rows joined with child1 rows.
    /// Preserve the order of rows of each of child1's chunks (between chunks and inside chunks).
    /// Return None when no more chunks are available in the right input.
    fn next(&mut self) -> Option<TableChunk> {
        if !self.open_bool {
            panic!("not opened");
        }
        let chunk1 = self.child1.next();
        if chunk1.is_none() {
            return None;
        }
        let chunk1 = chunk1.unwrap();

        let mut chunk0_final:TableChunk = vec![vec![]; self.left_columns];
        let mut chunk1_final:TableChunk = vec![vec![]; chunk1.len()];

        for (row, elem) in chunk1[self.join_column_idxs.1].iter().enumerate() {
            if !self.hash_map.contains_key(&elem) {
                continue;
            }

            let mut vec:Vec<Option<Value>> = Vec::new();
            for col in chunk1.iter() {
                if col.is_empty() {
                    vec.push(None);
                } else {
                    vec.push(Some(col[row].clone()));
                }
            }

            let left_record = self.hash_map.get(&elem).unwrap();
            add_record(&mut chunk0_final, left_record.clone());
            add_record(&mut chunk1_final, vec);
        }


        chunk0_final.append(&mut chunk1_final);
        Some(chunk0_final)
    }

    /// Cleans up the child operators and itself.
    fn close(&mut self) {
        self.child0.close();
        self.child1.close();
        self.open_bool = false;
        self.hash_map.clear();
    }
}

pub fn add_record(chunk: &mut TableChunk, vec: Vec<Option<Value>>) {
    for (idx, col) in chunk.iter_mut().enumerate() {
        let value = &vec[idx];
        match value {
            None => {},
            Some(v) => {
                col.push(v.clone());
            }
        }
    }
}
