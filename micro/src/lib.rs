#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]

mod api;
mod lba;
mod register;
pub use register::Register;
use serde::Deserialize;

use std::net::SocketAddr;

pub use api::{run as run_api_server, Intercepter, IntercepterType};
pub use lba::*;

#[derive(Debug)]
pub enum ServiceError {
    Other(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ServiceError")
    }
}

// [service] --> [endpoint] --> [address]
pub trait Service {
    fn name(&self) -> String;

    fn addr(&self) -> SocketAddr;

    fn lab(&self) -> LoadBalancerAlgorithm {
        return LoadBalancerAlgorithm::RoundRobin;
    }
}

#[derive(Debug)]
pub enum TransportError {
    Other(String),
}
// 中间传输层，可能还存在不合理的地方
pub trait Transport<'a, S, T>
where
    S: serde::Serializer,
    T: Deserialize<'a>,
{
    type Future<'b>: std::future::Future<Output = anyhow::Result<T, TransportError>>
    where
        Self: 'b;

    // default request josn implementation

    fn decode<'b>(&'b self, _s: S) -> Self::Future<'b> {
        unimplemented!()
    }
    fn encode<'r>(&'r self, _s: S) -> Self::Future<'r> {
        unimplemented!()
    }
}

pub struct Endpoint {
    addr: Vec<String>,
}

impl Endpoint {
    fn get_address(&self) -> Vec<String> {
        self.addr.clone()
    }
}

pub async fn make_service<T: Service>(s: T) -> T {
    let addr = s.addr();
    if addr.ip().is_loopback() {
        panic!("service address is loopback");
    }
    log::info!(
        "registry service is {} ip {}",
        s.name(),
        format!(
            "{}:{}",
            local_ip_address::local_ip().unwrap(),
            s.addr().port()
        )
    );

    let r = register::Register::default();

    if let Err(e) = r.register(&s).await {
        panic!("register service {} error {:?}", s.name(), e);
    }

    return s;
}
