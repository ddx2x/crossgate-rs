use crate::{make_executor, Register};
use crossbeam::sync::WaitGroup;
use futures::future::BoxFuture;

use tokio_context::context::Context;

pub trait Executor {
    fn group(&self) -> String; // register group name

    fn start<'a>(&self, ctx: Context, register: &'a Register) -> BoxFuture<'a, anyhow::Result<()>>;
}

pub async fn backend_service_run(e: impl Executor, p: plugin::PluginType) {
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
