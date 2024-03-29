#![feature(type_alias_impl_trait)]

mod api;
mod lba;
mod register;
mod task;
mod web;

pub use register::Register;
use serde::Deserialize;

use std::net::SocketAddr;

pub use api::{run as run_api_server, Intercepter, IntercepterType};
pub use lba::*;

pub use task::backend_service_run;
pub use task::Executor;

pub use web::{web_service_run, ServerRunFn};

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
pub trait Service: Sync + Send {
    fn name(&self) -> String;

    fn addr(&self) -> SocketAddr;

    fn lab(&self) -> LoadBalancerAlgorithm {
        dotenv::dotenv().ok();
        // if env STRICT is set, return strict
        let strict_address = ::std::env::var("STRICT").unwrap_or_else(|_| "".to_string());
        if !strict_address.is_empty() {
            return LoadBalancerAlgorithm::Strict(strict_address);
        }
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

#[derive(Debug)]
pub struct Endpoint {
    addr: Vec<String>,
}

impl Endpoint {
    fn get_address(&self) -> Vec<String> {
        self.addr.clone()
    }
}

pub async fn make_service<T>(s: T) -> T
where
    T: Service,
{
    if let Err(e) = register::Register::default().register_web_service(&s).await {
        panic!("register service {} error {:?}", s.name(), e);
    }

    return s;
}

pub async fn make_executor<'a, T>(s: &mut T) -> (&mut T, Register)
where
    T: Executor<'a>,
{
    let register = register::Register::default();
    if let Err(e) = register.register_backend_service(s).await {
        panic!("register backend service {} error {:?}", s.group(), e);
    }

    return (s, register);
}
