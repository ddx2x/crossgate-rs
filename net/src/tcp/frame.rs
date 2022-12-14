#[derive(Debug, Clone)]
pub enum FrameError {
    ParseError(String),
    Incomplete,
    Exit,
    Other(crate::NetError),
}

pub trait Frame: Send + Sync + Clone + 'static {
    fn read(&self, buf: &mut std::io::Cursor<&[u8]>) -> anyhow::Result<Self, FrameError>
    where
        Self: std::marker::Sized;

    fn write<W>(&self, w: &mut W) -> anyhow::Result<(), FrameError>
    where
        W: std::io::Write;
}
