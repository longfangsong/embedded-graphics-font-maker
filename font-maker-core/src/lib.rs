#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod format;
pub mod error;

#[cfg(test)]
mod tests {
    mod parser;
    mod data_size;
    mod header;
}
