use crate::{Connection, Handle, Handler};
use log;
use tokio::{net::TcpListener, sync::broadcast};

pub struct Listener {
    pub(crate) listener: TcpListener,
    pub(crate) notify_shutdown: broadcast::Sender<()>,
}

impl Listener {
    pub async fn run<H>(&mut self, h: H) -> crate::Result<()>
    where
        H: Handle,
    {
        loop {
            if let Ok((stream, addr)) = self.listener.accept().await {
                let mut handler = Handler {
                    inner: h.clone(),
                    connection: Connection::new(stream),
                    shutdown: self.notify_shutdown.subscribe(),
                };

                tokio::spawn(async move {
                    if let Err(err) = handler.run().await {
                        log::error!("connection client {:?} error {:?}", addr.to_string(), err);
                    }
                });
            }
        }
    }
}
