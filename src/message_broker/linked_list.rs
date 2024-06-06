#![allow(dead_code)]
use std::{ptr, sync::atomic::{AtomicPtr, Ordering}};

// use super::Client;

// #[cfg_attr(test, derive(Clone))]
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
    fn new() -> Self {
        let head =  AtomicPtr::new(ptr::null_mut());
        List { head }
    }

    fn append(&self, val: T) {
        let new_node = Box::into_raw(Box::new(AtmcNode::new(val)));
        
        loop {
            let head = self.head.load(Ordering::SeqCst);
            let mut cur = head;

            if head.is_null() {
                let compex = self
                    .head
                    .compare_exchange(ptr::null_mut(), new_node, Ordering::SeqCst, Ordering::SeqCst);
    
                if let Err(cnw) = compex { cur = cnw } 
                else { return }
            }
            
            if unsafe { iter_exchange(cur, new_node) } {
                return;
            }
        }
    }

    #[cfg(test)]
    unsafe fn collects(&self) -> Vec<T> {
        use std::mem;
        let mut v = vec![];
        let mut curr = self.head.load(Ordering::SeqCst);
        while !curr.is_null() {
            let cv = mem::transmute_copy::<T, T>(&(*curr).val);
            v.push(cv);
            curr = (*curr).next.load(Ordering::SeqCst);
        }
        v
    }
}

unsafe fn iter_exchange<T>(curptr: *mut AtmcNode<T>, excd: *mut AtmcNode<T>) -> bool {
    let mut curr = curptr;
    while !(*curr).next.load(Ordering::SeqCst).is_null() {
        curr = (*curr).next.load(Ordering::SeqCst);
    }

    let cmpx = (*curr).next.compare_exchange(
        ptr::null_mut(), 
        excd, 
        Ordering::SeqCst, 
        Ordering::SeqCst
    );

    cmpx.is_ok()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::{join, task::yield_now};

    use super::List;

    #[tokio::test]
    async fn concurrent_insert() {
        let list: Arc<List<u8>> = Arc::new(List::new());
        
        let ll2 = list.clone();
        let t1 = tokio::spawn(async move {
            for i in 0..6 {
                ll2.append(i);
            }
        });

        let ll2 = list.clone();
        let t2 = tokio::spawn(async move {
            for i in 0..6 {
                ll2.append(i);
            }
        });

        let ll2 = list.clone();
        let t3 = tokio::spawn(async move {
            for i in 0..6 {
                ll2.append(i);
            }
        });

        let _ = join!(t1, t2, t3);
        unsafe {
            let ppp = list.collects();
            println!("{:?}", ppp);
            assert!(ppp.len() == 18)
        }
    }

    #[tokio::test]
    async fn ctxswitch_insert() {
        let list: Arc<List<u8>> = Arc::new(List::new());
        
        let ll2 = list.clone();
        let t1 = tokio::spawn(async move {
            for i in 0..6 {
                ll2.append(i);
                yield_now().await
            }
        });

        let ll2 = list.clone();
        let t2 = tokio::spawn(async move {
            for i in 0..6 {
                ll2.append(i);
                yield_now().await
            }
        });

        let ll2 = list.clone();
        let t3 = tokio::spawn(async move {
            for i in 0..6 {
                ll2.append(i);
                yield_now().await
            }
        });

        let _ = join!(t1, t2, t3);
        unsafe {
            let ppp = list.collects();
            println!("{:?}", ppp);
            assert!(ppp.len() == 18)
        }
    }
}