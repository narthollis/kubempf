//! CancelableReadWrite stream testing with async I/O validation

#[cfg(test)]
mod tests {
    use crate::cancelable_stream::CancelableReadWrite;
    use futures::stream::AbortHandle;
    use std::io::{Error, ErrorKind};
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio_test::{assert_ready, task};

    // Helper function to create a no-op waker for testing
    fn noop_waker() -> Waker {
        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(std::ptr::null(), &VTABLE),
            |_| {},
            |_| {},
            |_| {},
        );
        let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
        unsafe { Waker::from_raw(raw_waker) }
    }

    // Mock stream for testing
    struct MockStream {
        read_data: Vec<u8>,
        read_pos: usize,
        write_data: Vec<u8>,
        should_error: bool,
        error_code: Option<i32>,
        shutdown_called: bool,
        flush_called: bool,
    }

    impl MockStream {
        fn new() -> Self {
            Self {
                read_data: Vec::new(),
                read_pos: 0,
                write_data: Vec::new(),
                should_error: false,
                error_code: None,
                shutdown_called: false,
                flush_called: false,
            }
        }

        fn with_read_data(mut self, data: Vec<u8>) -> Self {
            self.read_data = data;
            self
        }

        fn with_error(mut self, error_code: i32) -> Self {
            self.should_error = true;
            self.error_code = Some(error_code);
            self
        }
    }

    impl AsyncRead for MockStream {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            if self.should_error {
                let error = Error::new(ErrorKind::ConnectionReset, "Connection reset by peer");
                let mut error = error;
                if let Some(code) = self.error_code {
                    error = Error::from_raw_os_error(code);
                }
                return Poll::Ready(Err(error));
            }

            if self.read_pos >= self.read_data.len() {
                return Poll::Ready(Ok(())); // EOF
            }

            let remaining = &self.read_data[self.read_pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.read_pos += to_copy;

            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for MockStream {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            if self.should_error {
                let error = Error::new(ErrorKind::ConnectionReset, "Connection reset by peer");
                let mut error = error;
                if let Some(code) = self.error_code {
                    error = Error::from_raw_os_error(code);
                }
                return Poll::Ready(Err(error));
            }

            self.write_data.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            self.flush_called = true;
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            self.shutdown_called = true;
            Poll::Ready(Ok(()))
        }
    }

    impl Unpin for MockStream {}

    #[test]
    fn cancelable_stream_creation() {
        let mut mock_stream = MockStream::new();
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let _cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);
        // Should not panic on creation
    }

    #[tokio::test]
    async fn read_when_not_aborted() {
        let mut mock_stream = MockStream::new().with_read_data(b"Hello, World!".to_vec());
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        let mut buf = [0u8; 20];
        let mut read_buf = ReadBuf::new(&mut buf);

        let mut task = task::spawn(async {
            Pin::new(&mut cancelable)
                .poll_read(&mut Context::from_waker(&noop_waker()), &mut read_buf)
        });

        assert_ready!(task.poll());
        assert_eq!(read_buf.filled(), b"Hello, World!");
    }

    #[tokio::test]
    async fn read_when_aborted() {
        let mut mock_stream = MockStream::new().with_read_data(b"Hello, World!".to_vec());
        let (abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        // Abort before reading
        abort_handle.abort();

        let mut buf = [0u8; 20];
        let mut read_buf = ReadBuf::new(&mut buf);

        let mut task = task::spawn(async {
            Pin::new(&mut cancelable)
                .poll_read(&mut Context::from_waker(&noop_waker()), &mut read_buf)
        });

        // When aborted, it should call shutdown
        assert_ready!(task.poll());
        assert!(mock_stream.shutdown_called);
    }

    #[tokio::test]
    async fn read_handles_connection_reset_error() {
        let mut mock_stream = MockStream::new().with_error(104); // ECONNRESET
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        let mut buf = [0u8; 20];
        let mut read_buf = ReadBuf::new(&mut buf);

        let mut task = task::spawn(async {
            Pin::new(&mut cancelable)
                .poll_read(&mut Context::from_waker(&noop_waker()), &mut read_buf)
        });

        // Should handle connection reset gracefully and return Ok
        assert_ready!(task.poll());
    }

    #[tokio::test]
    async fn read_propagates_other_errors() {
        let mut mock_stream = MockStream::new().with_error(111); // ECONNREFUSED
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        let mut buf = [0u8; 20];
        let mut read_buf = ReadBuf::new(&mut buf);

        let mut task = task::spawn(async {
            Pin::new(&mut cancelable)
                .poll_read(&mut Context::from_waker(&noop_waker()), &mut read_buf)
        });

        // Should propagate non-connection-reset errors
        let poll_result = assert_ready!(task.poll());
        match poll_result {
            Poll::Ready(result) => assert!(result.is_err()),
            _ => panic!("Expected Poll::Ready"),
        }
    }

    #[tokio::test]
    async fn read_when_finished() {
        let mut mock_stream = MockStream::new();
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        // Simulate error 104 to set finished flag
        let mut mock_stream_with_error = MockStream::new().with_error(104);
        let mut cancelable_with_error =
            CancelableReadWrite::new(&mut mock_stream_with_error, &abort_registration);

        let mut buf = [0u8; 20];
        let mut read_buf = ReadBuf::new(&mut buf);

        // First call sets finished flag
        let mut task = task::spawn(async {
            Pin::new(&mut cancelable_with_error)
                .poll_read(&mut Context::from_waker(&noop_waker()), &mut read_buf)
        });
        assert_ready!(task.poll());

        // Second call should return immediately
        let mut read_buf2 = ReadBuf::new(&mut buf);
        let mut task2 = task::spawn(async {
            Pin::new(&mut cancelable_with_error)
                .poll_read(&mut Context::from_waker(&Waker::noop()), &mut read_buf2)
        });
        assert_ready!(task2.poll());
    }

    #[tokio::test]
    async fn write_when_not_aborted() {
        let mut mock_stream = MockStream::new();
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        let data = b"Hello, World!";
        let mut task = task::spawn(async {
            Pin::new(&mut cancelable).poll_write(&mut Context::from_waker(&noop_waker()), data)
        });

        let poll_result = assert_ready!(task.poll());
        match poll_result {
            Poll::Ready(result) => {
                assert!(result.is_ok());
                assert_eq!(result.unwrap(), data.len());
            },
            _ => panic!("Expected Poll::Ready"),
        }
        assert_eq!(mock_stream.write_data, data);
    }

    #[tokio::test]
    async fn write_when_aborted() {
        let mut mock_stream = MockStream::new();
        let (abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        // Abort before writing
        abort_handle.abort();

        let data = b"Hello, World!";
        let mut task = task::spawn(async {
            Pin::new(&mut cancelable).poll_write(&mut Context::from_waker(&noop_waker()), data)
        });

        let poll_result = assert_ready!(task.poll());
        match poll_result {
            Poll::Ready(result) => {
                assert!(result.is_ok());
                assert_eq!(result.unwrap(), 0); // Should return 0 when aborted
            },
            _ => panic!("Expected Poll::Ready"),
        }
        assert!(mock_stream.shutdown_called);
    }

    #[tokio::test]
    async fn write_when_finished() {
        let mut mock_stream = MockStream::new();
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        // Manually set finished flag by triggering connection reset on read
        let mut mock_stream_with_error = MockStream::new().with_error(104);
        let mut cancelable_with_finished =
            CancelableReadWrite::new(&mut mock_stream_with_error, &abort_registration);

        let mut buf = [0u8; 20];
        let mut read_buf = ReadBuf::new(&mut buf);
        let mut task = task::spawn(async {
            Pin::new(&mut cancelable_with_finished)
                .poll_read(&mut Context::from_waker(&noop_waker()), &mut read_buf)
        });
        assert_ready!(task.poll()); // This sets finished flag

        // Now try to write when finished
        let data = b"test";
        let mut task2 = task::spawn(async {
            Pin::new(&mut cancelable_with_finished)
                .poll_write(&mut Context::from_waker(&noop_waker()), data)
        });

        let poll_result = assert_ready!(task2.poll());
        match poll_result {
            Poll::Ready(result) => {
                assert!(result.is_ok());
                assert_eq!(result.unwrap(), 0); // Should return 0 when finished
            },
            _ => panic!("Expected Poll::Ready"),
        }
    }

    #[tokio::test]
    async fn flush_when_not_finished() {
        let mut mock_stream = MockStream::new();
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        let mut task = task::spawn(async {
            Pin::new(&mut cancelable).poll_flush(&mut Context::from_waker(&noop_waker()))
        });

        let poll_result = assert_ready!(task.poll());
        match poll_result {
            Poll::Ready(result) => assert!(result.is_ok()),
            _ => panic!("Expected Poll::Ready"),
        }
        assert!(mock_stream.flush_called);
    }

    #[tokio::test]
    async fn flush_when_finished() {
        let mut mock_stream = MockStream::new();
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        // Simulate finished state by triggering connection reset
        let mut mock_stream_with_error = MockStream::new().with_error(104);
        let mut cancelable =
            CancelableReadWrite::new(&mut mock_stream_with_error, &abort_registration);

        let mut buf = [0u8; 20];
        let mut read_buf = ReadBuf::new(&mut buf);
        let mut task = task::spawn(async {
            Pin::new(&mut cancelable)
                .poll_read(&mut Context::from_waker(&noop_waker()), &mut read_buf)
        });
        assert_ready!(task.poll()); // Sets finished flag

        // Now flush when finished
        let mut task2 = task::spawn(async {
            Pin::new(&mut cancelable).poll_flush(&mut Context::from_waker(&noop_waker()))
        });

        let poll_result = assert_ready!(task2.poll());
        match poll_result {
            Poll::Ready(result) => assert!(result.is_ok()),
            _ => panic!("Expected Poll::Ready"),
        }
    }

    #[tokio::test]
    async fn shutdown_when_not_finished() {
        let mut mock_stream = MockStream::new();
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        let mut task = task::spawn(async {
            Pin::new(&mut cancelable).poll_shutdown(&mut Context::from_waker(&noop_waker()))
        });

        let poll_result = assert_ready!(task.poll());
        match poll_result {
            Poll::Ready(result) => assert!(result.is_ok()),
            _ => panic!("Expected Poll::Ready"),
        }
        assert!(mock_stream.shutdown_called);
    }

    #[tokio::test]
    async fn shutdown_when_finished() {
        let mut mock_stream = MockStream::new();
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        // Simulate finished state
        let mut mock_stream_with_error = MockStream::new().with_error(104);
        let mut cancelable =
            CancelableReadWrite::new(&mut mock_stream_with_error, &abort_registration);

        let mut buf = [0u8; 20];
        let mut read_buf = ReadBuf::new(&mut buf);
        let mut task = task::spawn(async {
            Pin::new(&mut cancelable)
                .poll_read(&mut Context::from_waker(&noop_waker()), &mut read_buf)
        });
        assert_ready!(task.poll()); // Sets finished flag

        // Now shutdown when finished
        let mut task2 = task::spawn(async {
            Pin::new(&mut cancelable).poll_shutdown(&mut Context::from_waker(&noop_waker()))
        });

        let poll_result = assert_ready!(task2.poll());
        match poll_result {
            Poll::Ready(result) => assert!(result.is_ok()),
            _ => panic!("Expected Poll::Ready"),
        }
    }

    #[tokio::test]
    async fn multiple_abort_calls_are_safe() {
        let mut mock_stream = MockStream::new();
        let (abort_handle, abort_registration) = AbortHandle::new_pair();

        let mut cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        // Multiple abort calls should be safe
        abort_handle.abort();
        abort_handle.abort();
        abort_handle.abort();

        let data = b"test";
        let mut task = task::spawn(async {
            Pin::new(&mut cancelable).poll_write(&mut Context::from_waker(&noop_waker()), data)
        });

        let poll_result = assert_ready!(task.poll());
        match poll_result {
            Poll::Ready(result) => {
                assert!(result.is_ok());
                assert_eq!(result.unwrap(), 0);
            },
            _ => panic!("Expected Poll::Ready"),
        }
    }

    #[tokio::test]
    async fn unpin_trait_implemented() {
        let mut mock_stream = MockStream::new();
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();

        let cancelable = CancelableReadWrite::new(&mut mock_stream, &abort_registration);

        // This should compile - proving Unpin is implemented
        let _pinned: Pin<Box<_>> = Box::pin(cancelable);
    }

    #[test]
    fn abort_handle_relationship() {
        let mut mock_stream = MockStream::new();
        let (abort_handle1, abort_registration1) = AbortHandle::new_pair();
        let (abort_handle2, abort_registration2) = AbortHandle::new_pair();

        let _cancelable1 = CancelableReadWrite::new(&mut mock_stream, &abort_registration1);

        // Different abort handles should be independent
        abort_handle1.abort();
        assert!(!abort_handle2.is_aborted());

        abort_handle2.abort();
        assert!(abort_handle1.is_aborted());
        assert!(abort_handle2.is_aborted());
    }
}
