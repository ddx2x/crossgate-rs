use super::Listener;
use crate::Handle;
use futures::Future;
use tokio::{net::TcpListener, sync::broadcast};

pub async fn run<'a>(listener: TcpListener, h: impl Handle, shutdown: impl Future) {
    let (notify_shutdown, _) = broadcast::channel(16);

    let mut server = Listener {
        listener,
        notify_shutdown,
    };

    tokio::select! {
        _ = server.run(h) => {},
        _ = shutdown => {log::info!("shutdown !!")},
    }

    // let Listener {
    //     notify_shutdown, ..
    // } = server;
}
