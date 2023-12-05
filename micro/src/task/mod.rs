use crate::{make_executor, Register};
use crossbeam::sync::WaitGroup;
use futures::future::BoxFuture;
use plugin::PluginType;

use tokio_context::context::Context;

pub trait Executor<'a> {
    fn group(&self) -> String; // register group name

    fn start<'b>(
        &'a mut self,
        ctx: Context,
        register: &'b Register,
    ) -> BoxFuture<'b, anyhow::Result<()>>
    where
        'a: 'b;
}

pub async fn backend_service_run<'a, T>(e: &'a mut T, p: PluginType)
where
    T: Executor<'a> + Send + Sync + 'a,
{
    let (_, mut h) = Context::new();
    let wg = WaitGroup::new();

    let _ = plugin::init_plugin(
        h.spawn_ctx(),
        wg.clone(),
        plugin::ServiceType::BackendService,
        p,
    )
    .await;

    log::info!("backend service {} start", e.group());

    let (e, r) = make_executor(e).await;

    tokio::select! {
        _ = e.start(h.spawn_ctx(),&r)  => {},
        _ = tokio::signal::ctrl_c() => {
            h.cancel();
            wg.wait();
        },
    }
}
