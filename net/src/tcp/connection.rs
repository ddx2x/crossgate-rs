use bytes::{Buf, BytesMut};
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;

use crate::FrameError;

#[derive(Debug, Clone)]
pub enum ConnectionError {
    FrameIncomplete,
    FrameError(FrameError),
    IoError(String),
    Fin,
    Other(crate::NetError),
}

impl Error for ConnectionError {}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConnectionError")
    }
}
#[derive(Debug)]
pub struct Connection {
    // sufficient for our needs.
    pub(crate) stream: BufWriter<TcpStream>,
    // The buffer for reading frames.
    pub(crate) rb: BytesMut,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream: BufWriter::new(stream),
            rb: BytesMut::with_capacity(4 * 1024),
        }
    }

    pub async fn read_frame<F: super::Frame>(
        &mut self,
        frame: &F,
    ) -> Result<Option<F>, ConnectionError> {
        loop {
            match self.parse_frame(frame).await {
                Ok(item) => return Ok(item),
                Err(e) => match e {
                    ConnectionError::FrameIncomplete => {} // continue
                    _ => return Err(e),
                },
            }

            if let Err(e) = self.stream.read_buf(&mut self.rb).await {
                return Err(ConnectionError::IoError(e.to_string()));
            }
        }
    }

    async fn parse_frame<F: super::Frame>(
        &mut self,
        frame: &F,
    ) -> Result<Option<F>, ConnectionError> {
        let mut buf = std::io::Cursor::new(&self.rb[..]);
        let res = match frame.read(&mut buf) {
            Ok(item) => Ok(Some(item)),
            Err(e) => match e {
                crate::FrameError::Incomplete => Err(ConnectionError::FrameIncomplete),
                crate::FrameError::Exit => Err(ConnectionError::Fin), //fream is finished.
                _ => Err(ConnectionError::FrameError(e)),
            },
        };
        self.rb.advance(buf.position() as usize);
        res
    }

    pub async fn write_frame<F: super::Frame>(&mut self, frame: F) -> Result<(), ConnectionError> {
        let mut buf = std::io::Cursor::new(vec![]);
        if let Err(e) = frame.write(&mut buf) {
            return Err(ConnectionError::FrameError(e));
        }
        if let Err(e) = self.stream.write_all(&mut buf.get_ref()).await {
            return Err(ConnectionError::IoError(e.to_string()));
        }
        if let Err(e) = self.stream.flush().await {
            return Err(ConnectionError::IoError(e.to_string()));
        }
        Ok(())
    }
}