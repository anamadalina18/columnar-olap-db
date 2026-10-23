use std::any::Any;
use std::io::{Read, Seek, SeekFrom};
use std::rc::Rc;
use super::{Columns, DataFile, DataFileHeader, ReadDataFileHeader};
use crate::{DatabaseError, RowID, TableChunk, TypeID, Value};
use crate::DatabaseError::{EngineError, IOError, InvalidUTF8};

impl DataFile {
    pub fn new(header: DataFileHeader, data: TableChunk) -> Self {
        DataFile { header, data }
    }

    /// Serializes [`DataFile`] as specified in lab slides.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut vec:Vec<u8> = Vec::new();
        let mut vec_data:Vec<u8> = Vec::new();
        let mut vec_start_idx:Vec<usize> = Vec::new();
        let mut start_idx = self.header.byte_length();

        for i in 0..self.header.columns as usize {
            let vec_val = self.data.get(i).unwrap();
            let type_id = vec_val.get(0).unwrap().get_id();

            vec_start_idx.push(start_idx);

            match type_id {
                TypeID::Int => {
                    start_idx += 4 * self.header.rows as usize;

                    for j in 0..self.header.rows as usize {
                        let val = vec_val.get(j).unwrap();
                        let Value::Int(val) = vec_val.get(j).unwrap() else {panic!()};
                        vec_data.extend_from_slice(&val.to_le_bytes())
                    }
                },
                TypeID::UInt => {
                    start_idx += 4 * self.header.rows as usize;

                    for j in 0..self.header.rows as usize {
                        let val = vec_val.get(j).unwrap();
                        let Value::UInt(val) = vec_val.get(j).unwrap() else {panic!()};
                        vec_data.extend_from_slice(&val.to_le_bytes())
                    }
                },
                TypeID::RowID => {
                    start_idx += 8 * self.header.rows as usize;

                    for j in 0..self.header.rows as usize {
                        let val = vec_val.get(j).unwrap();
                        let Value::RowID(RowID(val)) = vec_val.get(j).unwrap() else {panic!()};
                        vec_data.extend_from_slice(&val.to_le_bytes())
                    }
                },
                TypeID::Varchar => {
                    start_idx += 8 * self.header.rows as usize;

                    for j in 0..self.header.rows as usize {
                        let val = vec_val.get(j).unwrap();
                        let Value::Varchar(val) = vec_val.get(j).unwrap() else {panic!()};
                        start_idx += val.len();
                        vec_data.extend_from_slice(&(val.len() as u64).to_le_bytes());
                        vec_data.extend_from_slice(val.as_bytes());
                    }
                }
            }
        }

        vec.extend_from_slice(&self.header.to_bytes(&vec_start_idx));
        vec.extend_from_slice(&vec_data);

        vec
    }

    /// Deserialize all columns into [`DataFile`] as specified in lab slides.
    pub fn parse<F: Seek + Read>(file: &mut F) -> Result<Self, DatabaseError> {
        let res = Self::parse_columns(file, Columns::All);

        match res {
            Ok(data) => Ok(data),
            Err(e) => panic!(),
        }
    }

    /// Deserialize selected columns into [`DataFile`] as specified in lab slides.
    /// Especially ensure that TableChunk.len() == ReadFileHeader.columns.
    pub fn parse_columns<F: Seek + Read>(
        file: &mut F,
        column_indexes: Columns,
    ) -> Result<Self, DatabaseError> {
        let mut data: TableChunk = TableChunk::new();
        let res_header = ReadDataFileHeader::parse(file);
        match res_header {
            Result::Ok(header) => {
                let mut column_idx = 0;
                for idx in header.column_info.clone() {
                    match &column_indexes {
                        Columns::All => {},
                        Columns::Selection(vec) => {
                            if !vec.contains(&column_idx) {
                                data.push(vec![]);
                                column_idx += 1;
                                continue;
                            }
                        }
                    }

                    file.seek(SeekFrom::Start(idx.1 as u64))?;
                    let mut vec_values:Vec<Value> = Vec::new();

                    for j in 0..header.rows {
                        match idx.0 {
                            TypeID::Int => {
                                let mut vec:[u8; 4] = [0u8; 4];
                                file.read_exact(&mut vec)?;
                                let val = i32::from_le_bytes(vec);

                                vec_values.push(Value::Int(val));
                            },
                            TypeID::UInt => {
                                let mut vec:[u8; 4] = [0u8; 4];
                                file.read_exact(&mut vec)?;
                                let val = u32::from_le_bytes(vec);

                                vec_values.push(Value::UInt(val));
                            },
                            TypeID::RowID => {
                                let mut vec:[u8; 8] = [0u8; 8];
                                file.read_exact(&mut vec)?;
                                let val = u64::from_le_bytes(vec);

                                vec_values.push(Value::RowID(RowID(val)));
                            },
                            TypeID::Varchar => {
                                let mut vec_length:[u8; 8] = [0u8; 8];
                                file.read_exact(&mut vec_length)?;
                                let len = u64::from_le_bytes(vec_length) as usize;

                                let mut vec_string = vec![0u8; len];
                                file.read_exact(&mut vec_string)?;
                                let val = Rc::new(String::from_utf8(vec_string.to_vec()).unwrap());

                                vec_values.push(Value::Varchar(val));
                            }
                        }
                    }

                    data.push(vec_values);

                    column_idx += 1;
                }

                return Ok(DataFile::new(header.into(), data))
            },
            Result::Err(e) => return Err(e),
        }
    }
}

impl DataFileHeader {
    const MAGIC: [u8; 8] = [0x53, 0x44, 0x4d, 0x53, 0x19, 0x03, 0x4a, 0x53];
    const HDR_LEN: usize = Self::MAGIC.len() + 16; // 8 byte for rows, 8 bytes for columns
    const COLUMN_INFO_LEN: usize = 16;

    pub fn new(rows: u64, columns: u64, column_info: Vec<TypeID>) -> Self {
        DataFileHeader {
            rows,
            columns,
            column_info,
        }
    }

    /// Serialize the header into a Vec<u8>.
    ///
    /// # Arguments
    ///
    /// * `column_start_indexes`: The column start indexes to write into the serialized header.
    ///
    /// returns: Vec<u8, Global>
    fn to_bytes(&self, column_start_indexes: &[usize]) -> Vec<u8> {
        let mut vec:Vec<u8> = Vec::new();

        vec.extend_from_slice(&Self::MAGIC);
        vec.extend_from_slice(&self.rows.to_le_bytes());
        vec.extend_from_slice(&self.columns.to_le_bytes());

        for i in 0..self.columns as usize {
            let val = *self.column_info.get(i).unwrap();

            match val {
                TypeID::Int => {
                    let x:usize = 0;
                    vec.extend_from_slice(&x.to_le_bytes());
                },
                TypeID::UInt => {
                    let x:usize = 1;
                    vec.extend_from_slice(&x.to_le_bytes());
                },
                TypeID::RowID => {
                    let x:usize = 2;
                    vec.extend_from_slice(&x.to_le_bytes());
                },
                TypeID::Varchar => {
                    let x:usize = 3;
                    vec.extend_from_slice(&x.to_le_bytes());
                }
            }

            let val_start_index = column_start_indexes.get(i).unwrap();
            vec.extend_from_slice(&val_start_index.to_le_bytes());
        }

        vec
    }

    /// Determine the byte length of the header. Especially useful in [DataFile::to_bytes].
    ///
    /// returns: usize
    fn byte_length(&self) -> usize {
        return Self::HDR_LEN + Self::COLUMN_INFO_LEN * self.columns as usize;
    }
}

impl ReadDataFileHeader {
    /// Parses the header of a data file including the column start indexes.
    ///
    /// # Arguments
    ///
    /// * `file`: The file to parse the header from.
    ///
    /// returns: Result<ReadDataFileHeader, DatabaseError>
    fn parse<F: Seek + Read>(file: &mut F) -> Result<Self, DatabaseError> {
        let mut magic_number:[u8; 8] = [0u8; 8];
        file.read_exact(&mut magic_number)?;

        if magic_number != DataFileHeader::MAGIC {
            return Err(EngineError)
        }

        let mut rows_vec:[u8; 8] = [0u8; 8];
        file.read_exact(&mut rows_vec)?;
        let rows = u64::from_le_bytes(rows_vec);

        let mut columns_vec:[u8; 8] = [0u8; 8];
        file.read_exact(&mut columns_vec)?;
        let columns = u64::from_le_bytes(columns_vec);

        let mut column_info: Vec<(TypeID, usize)> = Vec::with_capacity(columns as usize);
        for i in 0..columns as usize {
            let mut type_id_vec:[u8; 8] = [0u8; 8];
            file.read_exact(&mut type_id_vec)?;

            let mut val_vec:[u8; 8] = [0u8; 8];
            file.read_exact(&mut val_vec)?;
            let start_idx = usize::from_le_bytes(val_vec);

            let type_id = u64::from_le_bytes(type_id_vec);
            match TypeID::try_from(type_id) {
                Ok(type_id_val) => {
                    column_info.push((type_id_val, start_idx));
                },
                Err(err) => {
                    return Err(err);
                }
            }
        }

        Ok(ReadDataFileHeader {
            rows,
            columns,
            column_info
        })
    }
}
