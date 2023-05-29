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
use mdns_plugin::Mdns;
use thiserror::Error;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ServiceContent {
    pub service: String,
    pub lba: String,
    pub addr: String,
}

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("the plugin for key `{0}` is not available")]
    Error(String),
}

#[async_trait]
pub(crate) trait Plugin {
    async fn set(&mut self, k: &str, sc: ServiceContent) -> anyhow::Result<(), PluginError>;

    async fn get(&self, k: &str) -> anyhow::Result<Vec<ServiceContent>, PluginError>;

    async fn watch(&mut self);

    async fn refresh(&mut self, ctx: Context, wg: WaitGroup);
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
pub async fn init_plugin(ctx: Context, wg: WaitGroup, t: ServiceType, r#type: PluginType) {
    let plugin: Arc<Mutex<dyn Plugin + Send + Sync + 'static>> = match r#type {
        // PluginType::Etcd => Arc::new(Mutex::new(Etcd {})),
        PluginType::Mongodb => Arc::new(Mutex::new(Mongodb::new().await)),
        // _ => Arc::new(Mutex::new(Mdns {})),
        _ => panic!("not support plugin type"),
    };

    match t {
        ServiceType::ApiGateway => {
            plugin.lock().await.watch().await;
        }
        ServiceType::BackendService | ServiceType::WebService => {
            plugin.lock().await.refresh(ctx, wg).await;
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
pub async fn set(k: &str, val: ServiceContent) -> anyhow::Result<(), PluginError> {
    get_plugin().await.lock().await.set(k, val).await
}

#[inline]
pub async fn get(k: &str) -> anyhow::Result<Vec<crate::ServiceContent>, PluginError> {
    get_plugin().await.lock().await.get(k).await
}
