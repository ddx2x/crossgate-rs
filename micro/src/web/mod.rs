use std::net::SocketAddr;

use crossbeam::sync::WaitGroup;
use futures::future::BoxFuture;
use tokio_context::context::Context;

pub type ServerRunFn = for<'a> fn(addr: &'a SocketAddr) -> BoxFuture<'a, ()>;

pub async fn web_service_run<'a>(addr: &'a SocketAddr, srf: ServerRunFn, t: plugin::PluginType) {
    let (ctx, handle) = Context::new();
    let wg = WaitGroup::new();

    plugin::init_plugin(ctx, wg.clone(), plugin::ServiceType::WebService, t).await;

    tokio::select! {
        _ = srf(addr) => {},
        _ = tokio::signal::ctrl_c() => {
            handle.cancel();
            wg.wait();
        },
    }
}
