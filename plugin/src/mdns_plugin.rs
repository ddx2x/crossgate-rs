use crossbeam::sync::WaitGroup;
use tokio_context::context::Context;

pub struct Mdns {}

const SERVICE_NAME: &'static str = "_crossgate._tcp.local";

impl Mdns {
    pub fn new() -> Self {
        Mdns {}
    }
}

#[crate::async_trait]
impl crate::Plugin for Mdns {
    async fn set(&mut self, k: &str, val: crate::Content) -> Result<(), crate::PluginError> {
        log::info!("set key {},val {:?}", k, val);
        Ok(())
    }
    async fn get(&self, k: &str) -> Result<Vec<crate::Content>, crate::PluginError> {
        // 查询符合k的多个服务，返回Content 的 endpoints有一个或者多个
        Err(crate::PluginError::RecordNotFound)
    }

    async fn watch(&mut self) {}

    async fn renewal(&mut self, ctx: Context, wg: WaitGroup) {}
}
