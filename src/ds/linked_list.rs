use std::{mem::transmute, ops::Deref, ptr, sync::atomic::{AtomicPtr, Ordering}};

use crate::message_broker::cleanup::Cleanup;

struct AtomicOption<T> {
    inner: AtomicPtr<Option<T>>
}

impl<T> Default for AtomicOption<T> {
    fn default() -> Self {
        let opt_ptr = to_raw_boxed(Option::None);
        Self{
            inner: AtomicPtr::new(opt_ptr)
        }
    }
}

impl<T> AtomicOption<T> {
    fn take(&self) -> Option<T> {
        let inner_val = self.inner.load(Ordering::Acquire);
        unsafe {
            let actual_val = &mut *inner_val;
            actual_val.take()
        }
    }
}

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

pub struct DlistNode<T> {
    val: T,
    next: AtomicPtr<AtmcNode<T>>
}

impl<T> DlistNode<T> {
    fn new(val: T) -> Self {
        Self {
            val,
            next: AtomicPtr::default()
        }
    }
}


#[inline]
fn to_raw_boxed<T>(val: T) -> *mut T {
    let boxed = Box::new(val);
    Box::into_raw(boxed)
}

pub struct Dlist<T> {
    head: AtomicPtr<DlistNode<T>>,
    tail: AtomicPtr<AtomicPtr<DlistNode<T>>>
}

impl<T> Dlist<T> {
    pub fn new() -> Self {
        Self { 
            head: AtomicPtr::default(), 
            tail: AtomicPtr::default() 
        }
    }

    pub fn push(&self, val: T) {
        let new_pnode = to_raw_boxed(DlistNode::new(val));
        let head = self.head.load(Ordering::Acquire);
        
        if head.is_null() {
            self.head.store(new_pnode, Ordering::Relaxed);
            let p = to_raw_boxed(AtomicPtr::new(new_pnode));
            self.tail.store(p, Ordering::Release);
        }
    }

    pub fn pop(&self) -> Option<T> {
        let node = unsafe {
            let tail = self.tail.load(Ordering::Acquire);
            let inner = (*tail).swap(ptr::null_mut(), Ordering::Release);
            
            match inner.is_null() {
                true => return None,
                false => Box::from_raw(inner)
            }
        };

        Some(node.val)
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
    use super::{List, Dlist};


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

    // #[tokio::test]
    // async fn ctxswitch_insert() {
    //     let list: Arc<Dlist<u8>> = Arc::new(Dlist::new());
    //     async fn apeend(list: Arc<List<u8>>) {
    //         for i in 0..6 {
    //             list.append(i);
    //             yield_now().await
    //         }
    //     }

    //     let t1 = tokio::task::spawn(apeend(list.clone()));
    //     let t2 = tokio::task::spawn(apeend(list.clone()));
    //     let t3 = tokio::task::spawn(apeend(list.clone()));

    //     let _ = join!(t1, t2, t3);
    //     unsafe {
    //         let ppp = list.collects();
    //         println!("{:?}", ppp);
    //         assert!(ppp.len() == 18)
    //     }
    // }

    #[tokio::test(flavor = "multi_thread",  worker_threads = 3)]
    async fn concurrent_take_first() {
        let list: Arc<Dlist<u16>> = Arc::new(Dlist::new());
        // let output: Arc<Vec<u16>> = Arc::new(Vec::with_capacity(600));

        for i in 0..600 {
            list.push(i);
        }
        
        async fn take(list: Arc<Dlist<u16>>, _id: u8) {
            for _ in 0..200 {
                println!("{:?}", list.pop());
            }
        }

        let t1 = tokio::task::spawn(take(list.clone(), 1));
        let t2 = tokio::task::spawn(take(list.clone(), 2));
        let t3 = tokio::task::spawn(take(list.clone(), 3));
        let start = now();
        let _ = join!(t1, t2, t3);
        // let end = now();
        println!("start: {:?}", start);
        println!("elapsed: {:?}", start.elapsed());

    }

    fn now() -> SystemTime {
        SystemTime::now()
    }
}