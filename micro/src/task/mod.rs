use crate::{make_executor, Register};
use crossbeam::sync::WaitGroup;
use futures::future::BoxFuture;
use plugin::get_plugin_type;
use plugin::PluginType::Mongodb;

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

pub async fn backend_service_run<'a, T>(e: &'a mut T)
where
    T: Executor<'a> + Send + Sync + 'a,
{
    let (_, mut h) = Context::new();
    let wg = WaitGroup::new();

    let t = ::std::env::var("REGISTER_TYPE").unwrap_or_else(|_| Mongodb.as_str().into());

    let _ = plugin::init_plugin(
        h.spawn_ctx(),
        wg.clone(),
        plugin::ServiceType::BackendService,
        get_plugin_type(&t),
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
