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

    finished: bool,
}

impl<'a, T> CancelableReadWrite<'a, T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(stream: &'a mut T, abort_registration: &AbortRegistration) -> Self {
        Self {
            stream,
            abort: abort_registration.handle(),
            finished: false,
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
        if self.finished {
            return Poll::Ready(Ok(()));
        }
        let is_aborted = self.abort.is_aborted();
        let mut_self = self.get_mut();

        let pinned = Pin::new(&mut mut_self.stream);

        if is_aborted {
            pinned.poll_shutdown(cx)
        } else {
            match pinned.poll_read(cx, buf) {
                Poll::Ready(r) => match r {
                    Ok(()) => Poll::Ready(Ok(())),
                    Err(e) => {
                        // If the erroris 104 (Connection Reset by Peer) then we want to conceal the error
                        match e.raw_os_error() == Some(104) {
                            true => {
                                mut_self.finished = true;
                                Poll::Ready(Ok(()))
                            },
                            false => Poll::Ready(Err(e)),
                        }
                    },
                },
                Poll::Pending => Poll::Pending,
            }
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
        if self.finished {
            return Poll::Ready(Ok(0));
        }

        if self.abort.is_aborted() {
            Pin::new(&mut self.get_mut().stream)
                .poll_shutdown(cx)
                .map(|m| m.map(|_| 0))
        } else {
            Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        if self.finished {
            return Poll::Ready(Ok(()));
        }

        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        if self.finished {
            return Poll::Ready(Ok(()));
        }

        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}

impl<T> Unpin for CancelableReadWrite<'_, T> where T: AsyncRead + AsyncWrite + Unpin {}
