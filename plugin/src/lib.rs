// #![feature(generic_associated_types)]
// #![feature(type_alias_impl_trait)]
#![feature(is_some_with)]

use async_trait::async_trait;
use crossbeam::sync::WaitGroup;
use futures::lock::Mutex;
use std::sync::Arc;
use tokio_context::context::Context;

mod etcd;
use etcd::Etcd;

mod mongo;
use mongo::Mongodb;

mod mdns_plugin;
// #[cfg(any(plugin = "mdns", futures = "full"))]
use mdns_plugin::Mdns;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Content {
    pub service: String,
    pub lba: String,
    pub addr: String,
}

#[derive(Debug)]
pub enum PluginError {
    Error(String),
    RecordNotFound,
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::Error(s) => write!(f, "PluginError: {}", s),
            PluginError::RecordNotFound => write!(f, "PluginError: Key NotFound"),
        }
    }
}

#[async_trait]
pub(crate) trait Plugin {
    async fn set(&mut self, k: &str, val: Content) -> Result<(), PluginError>;

    async fn get(&self, k: &str) -> Result<Vec<Content>, PluginError>;

    async fn watch(&mut self);

    async fn renewal(&mut self, ctx: Context, wg: WaitGroup);
}

pub enum ServiceType {
    ApiGateway,
    BackendService,
    WebService,
}

pub enum PluginType {
    Etcd,
    Mongodb,
    Mdns,
}

pub fn get_plugin_type(name: &str) -> PluginType {
    match name {
        "etcd" => PluginType::Etcd,
        "mdns" => PluginType::Mdns,
        &_ => PluginType::Mongodb,
    }
}

impl PluginType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginType::Etcd => "etcd",
            PluginType::Mongodb => "mongodb",
            PluginType::Mdns => "mdns",
        }
    }
}

use once_cell::sync::OnceCell;

static PLUGIN: OnceCell<Arc<Mutex<dyn Plugin + Send + Sync + 'static>>> = OnceCell::new();

#[inline]
pub async fn init_plugin(ctx: Context, wg: WaitGroup, t: ServiceType, _type: PluginType) {
    let plugin: Arc<Mutex<dyn Plugin + Send + Sync + 'static>> = match _type {
        PluginType::Etcd => Arc::new(Mutex::new(Etcd {})),
        PluginType::Mongodb => Arc::new(Mutex::new(Mongodb::new().await)),
        _ => Arc::new(Mutex::new(Mdns {})),
    };

    match t {
        ServiceType::ApiGateway => {
            plugin.lock().await.watch().await;
        }
        ServiceType::BackendService | ServiceType::WebService => {
            plugin.lock().await.renewal(ctx, wg).await;
        }
    }

    _ = PLUGIN.set(plugin);
}

#[inline]
async fn get_plugin() -> &'static Arc<Mutex<dyn Plugin + Send + Sync>> {
    if PLUGIN.get().is_none() {
        panic!("plugin not init");
    }
    return PLUGIN.get().unwrap();
}

#[inline]
pub async fn set(k: &str, val: Content) -> Result<(), PluginError> {
    let mut plugin = get_plugin().await.lock().await;
    plugin.set(k, val).await
}

#[inline]
pub async fn get(k: &str) -> Result<Vec<crate::Content>, PluginError> {
    let plugin = get_plugin().await.lock().await;
    plugin.get(k).await
}
