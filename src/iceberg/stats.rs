use std::cmp::Ordering;
use super::*;
use crate::Value;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::RangeInclusive;

impl ColumnStats {
    pub fn contains_range(&self, range: RangeInclusive<Value>) -> bool {
        match self {
            ColumnStats::MinMax(variant) => variant.contains_range(range),
        }
    }
    pub fn contains(&self, value: Value) -> bool {
        match self {
            ColumnStats::MinMax(variant) => variant.contains(value),
        }
    }
}

impl MinMax {
    pub fn new(min: Value, max: Value) -> Self {
        if min > max {
            panic!("MinMax min must be less or equal max!");
        }
        Self { min, max }
    }

    /// Check if the range [self.min, self.max] overlaps with [range.start(), range.end()]
    pub fn contains_range(&self, range: RangeInclusive<Value>) -> bool {
        return (self.max >= *range.start() && self.max <= *range.end()) ||
            (self.min >= *range.start() && self.min <= *range.end()) ||
            (self.max >= *range.end() && self.min <= *range.start());
    }

    /// Check if value is in [self.min, self.max]
    pub fn contains(&self, value: Value) -> bool {
        return value >= self.min && value <= self.max;
    }
}

impl FileStats {
    pub fn new(column_stats: Vec<ColumnStats>) -> FileStats {
        FileStats { column_stats }
    }

    /// Check if the data in this file overlaps with the equality predicates. An equality predicate
    /// is represented as a tuple of the value to check for and the column ID as usize. Predicates
    /// are conjunctive. Thus, all predicates must match.
    pub fn contains(&self, predicates: Vec<(Value, usize)>) -> bool {
        for (val, idx) in predicates {
            let ColumnStats::MinMax(min_max) = self.column_stats.get(idx).unwrap();

            if !min_max.contains(val) {
                return false;
            }
        }

        true
    }

    /// Check if the data in this file overlaps with the range predicates. A range predicate is
    /// represented as a tuple of the range to check for and the column ID as usize. Predicates
    /// are conjunctive. Thus, all predicates must match.
    pub fn contains_range(&self, predicates: Vec<(RangeInclusive<Value>, usize)>) -> bool {
        for (val, idx) in predicates {
            let ColumnStats::MinMax(min_max) = self.column_stats.get(idx).unwrap();

            if !min_max.contains_range(val) {
                return false;
            }
        }

        true
    }
}
