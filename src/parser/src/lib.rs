#![feature(slice_partition_dedup)]
#![feature(fn_traits)]
#![feature(type_ascription)]
#![feature(unboxed_closures)]

mod array_parser;
mod code_parser;
mod is_empty_line;
mod parse_section;
mod section;
mod service;

pub use array_parser::*;
pub use is_empty_line::*;
pub use service::*;
