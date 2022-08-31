#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]


pub use micro::*;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
