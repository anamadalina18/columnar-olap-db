use crate::Value;
use std::cmp::Ordering;

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self {
            Value::Int(val) => {
                if let Value::Int(val_other) = other {
                    val.partial_cmp(val_other)
                } else {
                    panic!()
                }
            },
            Value::UInt(val) => {
                if let Value::UInt(val_other) = other {
                    val.partial_cmp(&val_other)
                } else {
                    panic!()
                }
            },
            Value::RowID(val) => {
                if let Value::RowID(val_other) = other {
                    val.partial_cmp(&val_other)
                } else {
                    panic!()
                }
            },
            Value::Varchar(val) => {
                if let Value::Varchar(val_other) = other {
                    val.partial_cmp(&val_other)
                } else {
                    panic!()
                }
            }
        }
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> Ordering{
        match self {
            Value::Int(val) => {
                if let Value::Int(val_other) = other {
                    val.cmp(val_other)
                } else {
                    panic!()
                }
            },
            Value::UInt(val) => {
                if let Value::UInt(val_other) = other {
                    val.cmp(&val_other)
                } else {
                    panic!()
                }
            },
            Value::RowID(val) => {
                if let Value::RowID(val_other) = other {
                    val.cmp(&val_other)
                } else {
                    panic!()
                }
            },
            Value::Varchar(val) => {
                if let Value::Varchar(val_other) = other {
                    val.cmp(&val_other)
                } else {
                    panic!()
                }
            }
        }
    }
}
