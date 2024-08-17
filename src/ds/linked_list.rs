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

pub struct DlistNode<T> {
    val: T,
    next: AtomicPtr<DlistNode<T>>,
    prev: AtomicPtr<DlistNode<T>>,
}

impl<T> DlistNode<T> {
    fn new(val: T) -> Self {
        Self {
            val,
            next: AtomicPtr::default(),
            prev: AtomicPtr::default()
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
    tail: AtomicPtr<DlistNode<T>>
}

impl<T> Dlist<T> {
    pub fn new() -> Self {
        Self { 
            head: AtomicPtr::default(),
            tail: AtomicPtr::default()
        }
    }

    pub fn push(&self, val: T) {
        let new_node = DlistNode::new(val);
        let new_head_ptr = to_raw_boxed(new_node);

        let head = self.head.load(Ordering::Acquire);
        while !self.push_logic(head, new_head_ptr) {}
    }

    fn push_logic(&self, head: *mut DlistNode<T>, new_head_ptr: *mut DlistNode<T>) -> bool {
        match head.is_null() {
            true => {
                let cmpx = self.head.compare_exchange_weak(
                    ptr::null_mut(), 
                    new_head_ptr, 
                    Ordering::AcqRel, 
                    Ordering::Relaxed
                );
    
                if cmpx.is_err(){
                    return false;
                }
    
                self.tail.store(new_head_ptr, Ordering::Release);
            }, false => {
                let cmpx = unsafe {
                    (*self.head.load(Ordering::Acquire)).prev.compare_exchange_weak(
                        ptr::null_mut(),
                        new_head_ptr,
                        Ordering::AcqRel,
                        Ordering::Relaxed
                    )
                };

                if cmpx.is_err() {
                    return false;
                }
            
                self.head.store(new_head_ptr, Ordering::Relaxed);
                unsafe {(*new_head_ptr).next.store(head, Ordering::Release)}
            }
        }
        true
    }

    pub fn pop(&self) -> Option<T> {
        loop {
            let tail_opt_ptr = self.tail.load(Ordering::Acquire);
            let prev = unsafe {
                if tail_opt_ptr.is_null() {
                    return None;
                }
    
                (*tail_opt_ptr).prev.load(Ordering::Acquire)
            };
    
            let cmpx = self.tail.compare_exchange_weak(
                tail_opt_ptr, 
                prev,
                Ordering::AcqRel, 
                Ordering::Relaxed
            );
    
            if let Ok(tcur_ptr) = cmpx {
                if prev.is_null() {
                    self.head.compare_exchange(
                        tcur_ptr, 
                        ptr::null_mut(), 
                        Ordering::Release, 
                        Ordering::Relaxed
                    ).unwrap();
                }
                let take_tail = unsafe { Box::from_raw(tcur_ptr) };
                return Some(take_tail.val);
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
    use tokio::join;
    use super::{List, Dlist};


    #[tokio::test(flavor = "multi_thread",  worker_threads = 3)]
    async fn concurrent_insert() {
        let list: Arc<List<u8>> = Arc::new(List::new());
        async fn apeend(list: Arc<List<u8>>) {
            println!("spawn task");
            for i in 0..200 {
                // print!("{}", i);
                list.append(i);
            }
        }
    
        let t1 = tokio::task::spawn(apeend(list.clone()));
        let t2 = tokio::task::spawn(apeend(list.clone()));
        let t3 = tokio::task::spawn(apeend(list.clone()));

        unsafe {
            let ppp = list.collects();
            println!("count: {}", ppp.len())
        }

        let start = now();
        let _ = join!(t1, t2, t3);
        println!("start: {:?}", start);
        println!("elapsed: {:?}", start.elapsed());
        
    }

    #[test]
    fn single_test() {
        let list: Dlist<u16> = Dlist::new();

        for i in 1..4 {
            for j in 1..i*2 {
                list.push(j);
                // println!("insert: {}", j);
            }
            
            while let Some(_v) = list.pop() {
                // println!("pop: {}", _v);
            }    
        }
    }

    #[tokio::test(flavor = "multi_thread",  worker_threads = 3)]
    async fn concurrent_pop() {
        let list: Arc<Dlist<u16>> = Arc::new(Dlist::new());

        for i in 0..600 {
            list.push(i);
        }
        
        async fn take(list: Arc<Dlist<u16>>, _id: u8) -> i32 {
            let mut cnt = 0;
            for _ in 0..200 {
                let _val = list.pop();
                if _val.is_some() {
                    cnt+=1;
                }
                // println!("worker: {}, {:?}", _id, _val);
            }
            cnt
        }

        let t1 = tokio::task::spawn(take(list.clone(), 1));
        let t2 = tokio::task::spawn(take(list.clone(), 2));
        let t3 = tokio::task::spawn(take(list.clone(), 3));
        let start = now();
        let (r1, r2, r3) = join!(t1, t2, t3);
        println!("start: {:?}", start);
        println!("elapsed: {:?}", start.elapsed());
        println!("r1: {}, r2: {}, r3: {}", r1.unwrap(), r2.unwrap(), r3.unwrap());

        println!("residu");
        while let Some(v) = list.pop() {
            println!("{:?}", v);
        }

    }

    #[tokio::test(flavor = "multi_thread",  worker_threads = 3)]
    async fn concurrent_push() {
        let list: Arc<Dlist<u16>> = Arc::new(Dlist::new());
        
        async fn insert(list: Arc<Dlist<u16>>, start: u16) {
            for i in 0..200 {
                list.push(start*i);
            }
        }

        let t1 = tokio::task::spawn(insert(list.clone(), 1));
        let t2 = tokio::task::spawn(insert(list.clone(), 2));
        let t3 = tokio::task::spawn(insert(list.clone(), 3));
        let start = now();
        let _ = join!(t1, t2, t3);

        let mut i = 0;
        while let Some(_v) = list.pop() {
            i += 1;
            // println!("{}. {:?}", i, _v)
        }

        println!("pop count: {}", i);
        println!("start: {:?}", start);
        println!("elapsed: {:?}", start.elapsed());

    }

    fn now() -> SystemTime {
        SystemTime::now()
    }
}