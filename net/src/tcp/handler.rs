use crate::{Connection, ConnectionError};
use tokio::sync::broadcast;

pub trait Handle: Sync + Send + Clone + 'static {
    type HandleFuture<'a>: futures::Future<Output = Result<(), ConnectionError>> + Send + Sync
    where
        Self: 'a;

    fn handle<'r>(self, conn: &'r mut Connection) -> Self::HandleFuture<'r>;
}

pub struct Handler<H>
where
    H: Handle,
{
    pub(crate) inner: H,
    pub(crate) connection: Connection,
    pub(crate) shutdown: broadcast::Receiver<()>,
}

impl<H> Handler<H>
where
    H: Handle,
{
    pub(crate) fn run<'a>(mut self) -> impl futures::Future<Output = anyhow::Result<()>> + 'a {
        async move {
            tokio::select! {
                res = self.inner.handle(&mut self.connection) => {
                    if let Err(ConnectionError::Fin) = res {
                            return Ok(());
                    }
                    return res.map_err(|e| e.into());
                },
                _ = self.shutdown.recv() => {Ok(())},
            }
        }
    }
}
