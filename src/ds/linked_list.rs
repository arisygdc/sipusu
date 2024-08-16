use std::{ptr, sync::atomic::{fence, AtomicPtr, Ordering}};

use crate::message_broker::cleanup::Cleanup;

    struct AtomicOption<T> {
        inner: AtomicPtr<Option<T>>,
    }

impl<T> Default for AtomicOption<T> {
    fn default() -> Self {
        let opt_ptr = to_raw_boxed(Option::None);
        Self::new(opt_ptr)
    }
}

impl<T> AtomicOption<T> {
    #[inline]
    fn new(p: *mut Option<T>) -> Self {
        Self{
            inner: AtomicPtr::new(p)
        }
    }

    fn take(&self) -> Option<T> {
        let inner_val: *mut Option<T> = self.inner.load(Ordering::Acquire);
        unsafe {
            let actual_val: &mut Option<T> = &mut *inner_val;
            actual_val.take()
        }
    }

    #[inline]
    fn compare_exchange(
        &self,
        current: *mut Option<T>,
        new: *mut Option<T>,
        success: Ordering,
        failure: Ordering,
    ) -> Result<*mut Option<T>, *mut Option<T>> {
        self.inner.compare_exchange(current, new, success, failure)
    }

    /// fail when option is Some(T)
    /// success will return true
    fn store(&self, val: T) -> bool {
        let vptr = to_raw_boxed(Some(val));
        let inner_val = self.inner.load(Ordering::Acquire);
        let inner = unsafe { &mut *inner_val };
        if let Some(_) = inner {
            return false;
        }

        self.valrpl_ptr(vptr);
        true
    }

    /// swap and free old ptr
    fn valrpl_and_freeptr(&self, p: *mut Option<T>) {
        let old_ptr = self.inner.swap(p, Ordering::Release);
        unsafe{ drop(Box::from_raw(old_ptr)) };
    }

    /// spap with new ptr
    #[inline]
    fn valrpl_ptr(&self, p: *mut Option<T>) -> *mut Option<T> {
        self.inner.swap(p, Ordering::Release)
    }

    #[inline]
    fn get_val_ptr(&self) -> *mut Option<T> {
        self.inner.load(Ordering::Acquire)
    }

    #[inline]
    fn load(&self, order: Ordering) -> *mut Option<T> {
        self.inner.load(order)
    }

    fn peek(&self) -> &Option<T> {
        let inner_val = self.inner.load(Ordering::Acquire);
        unsafe { &*inner_val }
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
    next: AtomicOption<DlistNode<T>>,
    prev: AtomicOption<DlistNode<T>>,
}

impl<T> DlistNode<T> {
    fn new(val: T) -> Self {
        Self {
            val,
            next: AtomicOption::default(),
            prev: AtomicOption::default()
        }
    }
}

#[inline]
fn to_raw_boxed<T>(val: T) -> *mut T {
    let boxed = Box::new(val);
    Box::into_raw(boxed)
}

pub struct Dlist<T> {
    head: AtomicOption<DlistNode<T>>,
    tail: AtomicOption<DlistNode<T>>
}

impl<T> Dlist<T> {
    pub fn new() -> Self {
        Self { 
            head: AtomicOption::default(),
            tail: AtomicOption::default()
        }
    }

    pub fn push(&self, val: T) {
        let new_node = DlistNode::new(val);
        let head = &self.head;

        if head.peek().is_none() {
            self.head.store(new_node);
            let p = self.head.get_val_ptr();
            self.tail.valrpl_and_freeptr(p);
            return;
        }

        let head_ptr = self.head.get_val_ptr();
        new_node.next.valrpl_and_freeptr(head_ptr);
        let new_node_ptr = to_raw_boxed(Some(new_node));
        self.head.valrpl_ptr(new_node_ptr);
        let head = unsafe { &*head_ptr };
        
        if let Some(vhead) = head {
            vhead.prev.valrpl_ptr(new_node_ptr);
        } else {
            panic!()
        }
    }

    pub fn pop(&self) -> Option<T> {
        let tail_opt_ptr = self.tail.get_val_ptr();
        let tail_opt = unsafe { &*tail_opt_ptr };
        let tail = match tail_opt {
            None => return None,
            Some(v) => v
        };
        
        let res = self.tail.compare_exchange(
            tail_opt_ptr, 
            tail.prev.get_val_ptr(), 
            Ordering::Release,
            Ordering::Relaxed
        );

        match res {
            Err(_) => None,
            Ok(garbage) => unsafe {
                let inner = *Box::from_raw(garbage);
                Some(inner?.val)
            }
        }
    }
}

impl<T> Drop for Dlist<T> {
    fn drop(&mut self) {
        while 
            let Some(_) = self.pop(){}
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

    #[test]
    fn single_test() {
        let list: Dlist<u16> = Dlist::new();

        for i in 1..4 {
            for j in 1..i*2 {
                list.push(j);
                println!("insert: {}", j);
            }
            
            while let Some(v) = list.pop() {
                println!("pop: {}", v);
            }    
        }
    }

    #[tokio::test(flavor = "multi_thread",  worker_threads = 3)]
    async fn concurrent_pop() {
        let list: Arc<Dlist<u16>> = Arc::new(Dlist::new());
        // let output: Arc<Vec<u16>> = Arc::new(Vec::with_capacity(600));

        for i in 0..600 {
            list.push(i);
        }
        
        async fn take(list: Arc<Dlist<u16>>, _id: u8) {
            // println!("task id {} start", _id);
            for _ in 0..200 {
                println!("worker: {}, {:?}", _id, list.pop());
            }
            // println!("task id {} finish", _id);
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