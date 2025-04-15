use crossbeam::sync::WaitGroup;
use futures::future::BoxFuture;
use plugin::{get_plugin_type, PluginType::Mongodb};
use std::net::SocketAddr;
use tokio_context::context::Context;

pub type ServerRunFn = for<'a> fn(addr: &'a SocketAddr) -> BoxFuture<'a, ()>;

pub async fn web_service_run<'a>(addr: &'a SocketAddr, srf: ServerRunFn) {
    let (ctx, handle) = Context::new();
    let wg = WaitGroup::new();

    let t = ::std::env::var("REGISTER_TYPE").unwrap_or_else(|_| Mongodb.as_str().into());

    plugin::init_plugin(
        ctx,
        wg.clone(),
        plugin::PluginServiceType::WebService,
        get_plugin_type(&t),
    )
    .await;

    tokio::select! {
        _ = srf(addr) => {},
        _ = tokio::signal::ctrl_c() => {
            handle.cancel();
            wg.wait();
        },
    }
}
