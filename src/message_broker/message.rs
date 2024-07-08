use std::{future::Future, io, marker::PhantomPinned, pin::Pin, ptr, sync::{atomic::{AtomicPtr, Ordering}, Arc}, task::{Context, Poll, Waker}};

use pin_project_lite::pin_project;

use crate::{ds::{linked_list::List, GetFromQueue, InsertQueue}, protocol::v5::publish::PublishPacket};
use super::{cleanup::Cleanup, client::clobj::ClientID};

#[derive(Default)]
pub struct Message {
    pub publisher: Option<ClientID>,
    pub packet: PublishPacket
}

pub struct Queue{
    inner: Arc<MQueue>
}

impl Queue {
    pub fn new() -> Self {
        Self { inner: Arc::new(MQueue::new()) }
    }
}

impl Clone for Queue {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

impl Cleanup for  Queue {
    async fn clear(self) {
        while let Some(msg) = self.inner.q.take_first() {
            println!("Drop message");
            drop(msg)
        }
    }
}

impl InsertQueue<Message> for Queue {
    fn enqueue(&self, val: Message) {
        self.inner.q.append(val);
        let waker = self.inner.s.swap(ptr::null_mut(), Ordering::Acquire);
        if !waker.is_null() {
            unsafe {
                Box::from_raw(waker)
                .wake()
            };
        }
    }
}

impl GetFromQueue<Message> for Queue {
    async fn dequeue(&self) -> io::Result<Message> {
        self.inner.get().await
    }
}


impl MQueue {
    fn get<'a>(&'a self) -> DequeueMessage {
        DequeueMessage {
            queue: self,
            _marker: PhantomPinned
        }
    }
}

pub struct MQueue {
    q: List<Message>,
    s: AtomicPtr<Waker>,
}

impl MQueue {
    pub(super) fn new() -> Self {
        Self { q: List::new(), s: AtomicPtr::new(ptr::null_mut()) }
    }
}

pin_project! {
    pub(super) struct DequeueMessage<'a> {
        queue: &'a MQueue,
        #[pin]
        _marker: PhantomPinned,
    }
}

impl Future for DequeueMessage<'_> {
    type Output = io::Result<Message>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();
        if let Some(msg) = me.queue.q.take_first() {
            return Poll::Ready(Ok(msg));
        }

        let waker = Box::into_raw(Box::new(cx.waker().clone()));
        let old = me.queue.s.swap(waker, Ordering::Release);
        if !old.is_null() {
            drop(unsafe {
                Box::from_raw(old)
            })
        }
        Poll::Pending
    }
}