use crate::stream::Fuse;
use futures_core::stream::{Stream, FusedStream};
use futures_core::task::{Context, Poll};
#[cfg(feature = "sink")]
use futures_sink::Sink;
use pin_project::{pin_project, project};
use core::mem;
use core::pin::Pin;
use alloc::vec::Vec;

/// Stream for the [`chunks`](super::StreamExt::chunks) method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Chunks<St: Stream> {
    #[pin]
    stream: Fuse<St>,
    items: Vec<St::Item>,
    cap: usize, // https://github.com/rust-lang/futures-rs/issues/1475
}

impl<St: Stream> Chunks<St> where St: Stream {
    pub(super) fn new(stream: St, capacity: usize) -> Chunks<St> {
        assert!(capacity > 0);

        Chunks {
            stream: super::Fuse::new(stream),
            items: Vec::with_capacity(capacity),
            cap: capacity,
        }
    }

    fn take(self: Pin<&mut Self>) -> Vec<St::Item> {
        let cap = self.cap;
        mem::replace(self.project().items, Vec::with_capacity(cap))
    }

    delegate_access_inner!(stream, St, (.));
}

impl<St: Stream> Stream for Chunks<St> {
    type Item = Vec<St::Item>;

    #[project]
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        #[project]
        let Chunks { mut stream, items, cap } = self.as_mut().project();
        loop {
            match ready!(stream.as_mut().poll_next(cx)) {
                // Push the item into the buffer and check whether it is full.
                // If so, replace our buffer with a new and empty one and return
                // the full one.
                Some(item) => {
                    items.push(item);
                    if items.len() >= *cap {
                        return Poll::Ready(Some(self.take()))
                    }
                }

                // Since the underlying stream ran out of values, return what we
                // have buffered, if we have anything.
                None => {
                    let last = if items.is_empty() {
                        None
                    } else {
                        let full_buf = mem::replace(items, Vec::new());
                        Some(full_buf)
                    };

                    return Poll::Ready(last);
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let chunk_len = if self.items.is_empty() { 0 } else { 1 };
        let (lower, upper) = self.stream.size_hint();
        let lower = lower.saturating_add(chunk_len);
        let upper = match upper {
            Some(x) => x.checked_add(chunk_len),
            None => None,
        };
        (lower, upper)
    }
}

impl<St: FusedStream> FusedStream for Chunks<St> {
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated() && self.items.is_empty()
    }
}

// Forwarding impl of Sink from the underlying stream
#[cfg(feature = "sink")]
impl<S, Item> Sink<Item> for Chunks<S>
where
    S: Stream + Sink<Item>,
{
    type Error = S::Error;

    delegate_sink!(stream, Item);
}
