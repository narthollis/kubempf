use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::{AbortHandle, AbortRegistration};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct CancelableReadWrite<'a, T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    stream: &'a mut T,
    abort: AbortHandle,
}

impl<'a, T> CancelableReadWrite<'a, T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(stream: &'a mut T, abort_registration: &AbortRegistration) -> Self {
        Self {
            stream,
            abort: abort_registration.handle(),
        }
    }
}

impl<'a, T> AsyncRead for CancelableReadWrite<'a, T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.abort.is_aborted() {
            Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
        } else {
            Pin::new(&mut self.get_mut().stream).poll_read(cx, buf)
        }
    }
}

impl<'a, T> AsyncWrite for CancelableReadWrite<'a, T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        if self.abort.is_aborted() {
            Pin::new(&mut self.get_mut().stream)
                .poll_shutdown(cx)
                .map(|m| m.map(|_| 0))
        } else {
            Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}

impl<T> Unpin for CancelableReadWrite<'_, T> where T: AsyncRead + AsyncWrite + Unpin {}
