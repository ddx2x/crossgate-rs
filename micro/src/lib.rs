#![feature(type_alias_impl_trait)]

mod api;
mod lba;
mod register;
mod task;
mod web;

pub use register::Register;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;

pub use api::{run as run_api_server, Intercepter, IntercepterType};
pub use lba::*;
pub use task::{backend_service_run, Executor};
pub use web::{web_service_run, ServerRunFn};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Service error: {0}")]
    Other(String),
    #[error("Transport error: {0}")]
    Transport(String),
}

// [service] --> [endpoint] --> [address]
pub trait Service: Sync + Send {
    fn name(&self) -> String;
    fn addr(&self) -> SocketAddr;
    fn load_balancer_algorithm(&self) -> LoadBalancerAlgorithm {
        LoadBalancerAlgorithm::from_environment()
    }
}

// 中间传输层
pub trait Transport<'a, S, T>
where
    S: serde::Serializer,
    T: Deserialize<'a>,
{
    type Future<'b>: std::future::Future<Output = Result<T, ServiceError>>
    where
        Self: 'b;

    fn decode<'b>(&'b self, serializer: S) -> Self::Future<'b>;
    fn encode<'r>(&'r self, serializer: S) -> Self::Future<'r>;
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    addresses: Arc<Vec<String>>,
}

impl Endpoint {
    pub fn new(addresses: Vec<String>) -> Self {
        Self {
            addresses: Arc::new(addresses),
        }
    }

    pub fn get_service_addresses(&self) -> &[String] {
        &self.addresses
    }

    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }
}

pub async fn register_service<T>(service: T) -> T
where
    T: Service,
{
    if let Err(e) = register::Register::default().register_web_service(&service).await {
        panic!("Failed to register service {}: {:?}", service.name(), e);
    }
    service
}

pub async fn register_executor<'a, T>(executor: &mut T) -> (&mut T, Register)
where
    T: Executor<'a>,
{
    let register = register::Register::default();
    if let Err(e) = register.register_backend_service(executor).await {
        panic!("Failed to register backend service {}: {:?}", executor.group(), e);
    }
    (executor, register)
}
