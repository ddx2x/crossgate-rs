use std::sync::Arc;

use async_trait::async_trait;
use crossbeam::sync::WaitGroup;

use tokio::sync::Mutex;
use tokio_context::context::Context;

mod etcd;
use etcd::Etcd;

mod mongo;
use mongo::MongodbPlugin;

mod mdns_plugin;
use mdns_plugin::Mdns;
use thiserror::Error;

pub enum PluginType {
    Etcd,
    Mongodb,
    Mdns,
}

pub fn get_plugin_type(name: &str) -> PluginType {
    let name = name.to_lowercase();
    match name.as_str() {
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ServiceContent {
    pub service: String,
    pub lba: String,
    pub addr: String,
    pub r#type: i32, // 1:web service ,2:backend service
}

impl Default for ServiceContent {
    fn default() -> Self {
        ServiceContent {
            service: "".to_string(),
            lba: "".to_string(),
            addr: "".to_string(),
            r#type: 1,
        }
    }
}

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("the plugin for key `{0}` is not available")]
    Error(String),
}

#[async_trait]
pub trait Plugin {
    async fn register_service(
        &self,
        key: &str,
        service_content: ServiceContent,
    ) -> anyhow::Result<()>;

    async fn get_web_service(&self, key: &str) -> anyhow::Result<Vec<ServiceContent>>;

    async fn get_backend_service(&self, key: &str) -> anyhow::Result<(String, Vec<String>)>;
}

#[async_trait]
pub(crate) trait Synchronize {
    async fn cache_refresh(&mut self);
    async fn remote_refresh(&mut self, ctx: Context, wg: WaitGroup);
    async fn twoway_refresh(&mut self, ctx: Context, wg: WaitGroup);
}

pub enum ServiceType {
    ApiGateway,
    BackendService,
    WebService,
}

use once_cell::sync::OnceCell;

static PLUGIN: OnceCell<Box<dyn Plugin + Send + Sync + 'static>> = OnceCell::new();

#[inline]
pub async fn init_plugin(
    ctx: Context,
    wg: WaitGroup,
    svc_type: ServiceType,
    r#type: PluginType,
) -> Box<dyn Plugin + Send + Sync + 'static> {
    let mut plugin = match r#type {
        PluginType::Mongodb => MongodbPlugin::new().await,
        _ => panic!("not support plugin type"),
    };

    match svc_type {
        ServiceType::ApiGateway => {
            plugin.cache_refresh().await;
        }
        ServiceType::BackendService => {
            plugin.twoway_refresh(ctx, wg).await;
        }
        ServiceType::WebService => {
            plugin.remote_refresh(ctx, wg).await;
        }
    }

    _ = PLUGIN.set(Box::new(plugin.clone()));

    Box::new(plugin)
}

#[inline]
async fn plugin_instance() -> &'static Box<dyn Plugin + Send + Sync> {
    if PLUGIN.get().is_none() {
        panic!("plugin not init");
    }
    return PLUGIN.get().unwrap();
}

#[inline]
pub async fn register_service(key: &str, service_content: ServiceContent) -> anyhow::Result<()> {
    plugin_instance()
        .await
        .register_service(key, service_content)
        .await
}

#[inline]
pub async fn get_web_service(k: &str) -> anyhow::Result<Vec<ServiceContent>> {
    plugin_instance().await.get_web_service(k).await
}

#[inline]
pub async fn get_backend_service(k: &str) -> anyhow::Result<(String, Vec<String>)> {
    plugin_instance().await.get_backend_service(k).await
}
