use crate::async_trait;
use crossbeam::sync::WaitGroup;
use tokio_context::context::Context;

pub struct NonePlugin;
impl NonePlugin {
    pub(super) async fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl super::Plugin for NonePlugin {
    async fn register_service(
        &self,
        _key: &str,
        _service_content: super::ServiceContent,
    ) -> anyhow::Result<()> {
        Box::pin(async move { Ok(()) }).await
    }

    async fn get_web_service(&self, _key: &str) -> anyhow::Result<Vec<super::ServiceContent>> {
        Box::pin(async move { Ok(vec![]) }).await
    }

    async fn get_backend_service(&self, _key: &str) -> anyhow::Result<(String, Vec<String>)> {
        Box::pin(async move { Ok((String::new(), vec![])) }).await
    }
}

#[async_trait]
impl super::Synchronize for NonePlugin {
    async fn gateway_service_handle(&mut self) {}
    async fn backend_service_handle(&mut self, ctx: Context, wg: WaitGroup) {
        let mut ctx = ctx;
        tokio::spawn(async move {
            tokio::select! {
                _ = async move {
                    // tokio sleep 10s
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    }
                } =>{},
                _ = ctx.done() => {
                    drop(wg.clone());
                    return;
                }
            }
        });
    }
    async fn web_service_handle(&mut self, _ctx: Context, _wg: WaitGroup) {}
}
