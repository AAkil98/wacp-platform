use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tonic::Streaming;

use wacp_transport::wacp_v1;

use crate::error::Error;

/// Async stream of envelopes from the inbox.
pub struct InboxStream {
    inner: Streaming<wacp_v1::Envelope>,
}

impl InboxStream {
    pub(crate) fn new(inner: Streaming<wacp_v1::Envelope>) -> Self {
        Self { inner }
    }
}

impl Stream for InboxStream {
    type Item = Result<wacp_v1::Envelope, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(envelope))) => Poll::Ready(Some(Ok(envelope))),
            Poll::Ready(Some(Err(status))) => Poll::Ready(Some(Err(Error::RpcFailed(status)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Async stream of commands from the coordinator.
pub struct CommandStream {
    inner: Streaming<wacp_v1::Command>,
}

impl CommandStream {
    pub(crate) fn new(inner: Streaming<wacp_v1::Command>) -> Self {
        Self { inner }
    }
}

impl Stream for CommandStream {
    type Item = Result<wacp_v1::Command, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(cmd))) => Poll::Ready(Some(Ok(cmd))),
            Poll::Ready(Some(Err(status))) => Poll::Ready(Some(Err(Error::RpcFailed(status)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
