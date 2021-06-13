#![feature(slice_partition_dedup)]
#![feature(type_ascription)]

mod array_parser;
mod is_empty_line;
mod parse_section;
mod section;
mod service;

pub use array_parser::*;
pub use is_empty_line::*;
pub use service::*;
