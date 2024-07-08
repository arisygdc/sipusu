use std::{ptr, sync::atomic::{AtomicPtr, Ordering}};

use crate::message_broker::cleanup::Cleanup;

pub struct AtmcNode<T> {
    val: T,
    next: AtomicPtr<AtmcNode<T>>
}

impl<T> AtmcNode<T> {
    fn new(val: T) -> Self {
        Self {
            val,
            next: AtomicPtr::new(ptr::null_mut()),
        }
    }
}

pub struct List<T> {
    head: AtomicPtr<AtmcNode<T>>
}

impl<T> List<T> {
    pub fn new() -> Self {
        let head =  AtomicPtr::new(ptr::null_mut());
        List { head }
    }

    /// insert on last element
    pub fn append(&self, val: T) {
        let new_node = Box::into_raw(Box::new(AtmcNode::new(val)));
        
        loop {
            let head = self.head.load(Ordering::Acquire);

            if head.is_null() {
                let compex = self
                    .head
                    .compare_exchange(ptr::null_mut(), new_node, Ordering::SeqCst, Ordering::SeqCst);
                
                match compex {
                    Err(_) => continue,
                    Ok(_) => return
                }
            }
            
            if unsafe { iter_exchange(self.head.load(Ordering::SeqCst), new_node) } {
                return;
            }
        }
    }

    #[cfg(test)]
    unsafe fn collects(&self) -> Vec<T> {
        use std::mem;
        let mut collect = vec![];
        let mut curr = self.head.load(Ordering::SeqCst);
        while !curr.is_null() {
            let cv = mem::transmute_copy::<T, T>(&(*curr).val);
            collect.push(cv);
            curr = (*curr).next.load(Ordering::SeqCst);
        }
        collect
    }
}

impl<T> List<T> {
    pub fn take_first(&self) -> Option<T> {
        let head = self.head.load(Ordering::Acquire);
        if head.is_null() {
            return None;
        }
        
        unsafe {
            let next = (*head).next.load(Ordering::Acquire);
            self.head.compare_exchange(
                head, 
                next, 
                Ordering::Release, 
                Ordering::Relaxed
            ).ok()?
        };
        let cast = unsafe{Box::from_raw(head)};
        Some(cast.val)
    }
}

// TODO: cleanup linked list
impl<T> Cleanup for List<T> {
    async fn clear(self) {
        println!("TODO: clear linked list");
    }
}

unsafe fn iter_exchange<T>(curptr: *mut AtmcNode<T>, excd: *mut AtmcNode<T>) -> bool {
    let mut curr = curptr;
    while !(*curr).next.load(Ordering::Acquire).is_null() {
        curr = (*curr).next.load(Ordering::Acquire);
    }

    let cmpx = (*curr).next.compare_exchange(
        ptr::null_mut(), 
        excd, 
        Ordering::Release, 
        Ordering::Relaxed
    );

    cmpx.is_ok()
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::SystemTime};
    use tokio::{join, task::yield_now};

    use super::List;

    #[tokio::test(flavor = "multi_thread",  worker_threads = 3)]
    async fn concurrent_insert() {
        let list: Arc<List<u8>> = Arc::new(List::new());
        async fn apeend(list: Arc<List<u8>>) {
            println!("spawn task");
            for i in 0..6 {
                print!("{}", i);
                list.append(i);
            }
        }
    
        let t1 = tokio::task::spawn(apeend(list.clone()));
        let t2 = tokio::task::spawn(apeend(list.clone()));
        let t3 = tokio::task::spawn(apeend(list.clone()));

        let _ = join!(t1, t2, t3);
        unsafe {
            let ppp = list.collects();
            assert!(ppp.len() == 18)
        }
    }

    #[tokio::test]
    async fn ctxswitch_insert() {
        let list: Arc<List<u8>> = Arc::new(List::new());
        async fn apeend(list: Arc<List<u8>>) {
            for i in 0..6 {
                list.append(i);
                yield_now().await
            }
        }

        let t1 = tokio::task::spawn(apeend(list.clone()));
        let t2 = tokio::task::spawn(apeend(list.clone()));
        let t3 = tokio::task::spawn(apeend(list.clone()));

        let _ = join!(t1, t2, t3);
        unsafe {
            let ppp = list.collects();
            println!("{:?}", ppp);
            assert!(ppp.len() == 18)
        }
    }

    #[tokio::test(flavor = "multi_thread",  worker_threads = 3)]
    async fn concurrent_take_first() {
        let list: Arc<List<u16>> = Arc::new(List::new());
        for i in 0..600 {
            list.append(i);
        }
        
        async fn take(list: Arc<List<u16>>, _id: u8) {
            for _ in 0..200 {
                list.take_first();
            }
        }

        let t1 = tokio::task::spawn(take(list.clone(), 1));
        let t2 = tokio::task::spawn(take(list.clone(), 2));
        let t3 = tokio::task::spawn(take(list.clone(), 3));
        let start = now();
        let _ = join!(t1, t2, t3);
        // let end = now();
        println!("start: {:?}", start);
        // println!("end: {:?}", end);
        println!("elapsed: {:?}", start.elapsed());

    }

    fn now() -> SystemTime {
        SystemTime::now()
    }
}