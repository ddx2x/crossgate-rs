#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]

pub mod http;
pub mod tcp;

pub use http::*;
pub use tcp::*;

use std::error::Error;

#[derive(Debug, Clone)]
pub enum NetError {
    InternalError(String),
}

impl Error for NetError {}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NetError")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {}
}
