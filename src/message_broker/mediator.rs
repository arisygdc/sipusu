use std::{sync::Arc, time::Duration};
use tokio::{io, task::JoinHandle, time};
use crate::protocol::mqtt::PublishPacket;
use super::{client::{Client, ClientID, UpdateClient}, clients::Clients, linked_list::List, provider::EventHandler, trie::Trie, Consumer, Event, EventListener, Messanger};

pub struct BrokerMediator {
    clients: Clients,
    message_queue: Arc<List<PublishPacket>>,
    router: Arc<Trie<ClientID>>,
}

impl BrokerMediator {
    pub fn new() -> Self {
        let clients = Clients::new();
        let message_queue = Arc::new(List::new());
        let router = Arc::new(Trie::new());
        Self{ clients, message_queue, router }
    }
}

impl<'cp> BrokerMediator {
    pub async fn register<CB, R>(&self, new_cl: Client, callback: CB) -> io::Result<R>
    where CB: FnOnce(&'cp mut Client) -> R
    {
        let clid = new_cl.clid.clone();
        println!("[register] client {:?}", clid);
        self.clients.insert(new_cl).await;
        println!("inserted");
        let result = self.clients.search_mut_client(&clid, callback)
            .await
            .ok_or(io::Error::new(
                io::ErrorKind::Other, 
                format!("cannot inserting client {}", clid)
            ))?;
        println!("registered");
        Ok(result)
    }

    /// wakeup session
    /// replacing old socket with incoming socket connection 
    pub async fn try_restore_connection<CB, R>(&self, clid: &ClientID, bucket: &mut UpdateClient, callback: CB) -> Result<R, String> 
    where CB: FnOnce(&'cp mut Client) -> R
    {
        let res = match self.clients.search_mut_client(clid, |c| {
            match c.restore_connection(bucket) {
                Ok(_) => (),
                Err(e) => return Err(e.to_string())
            };
            Ok(callback(c))
        }).await 
        {
            None => return Err(String::from("Not found")),
            Some(res) => res
        };
        res
    }

    pub async fn session_exists(&self, clid: &ClientID) -> bool {
        self.clients.session_exists(clid).await
    }

    pub async fn remove(&self, clid: &ClientID) -> Result<(), String> {
        self.clients.remove(clid).await
    }

    pub fn join_handle(&mut self) -> (JoinHandle<()>, JoinHandle<()>) {
        let clients = self.clients.clone();
        let action = EventHandler::from(
            self.message_queue.clone(), 
            self.router.clone()
        );
    
        let t1 = tokio::task::spawn(event_listener(clients.clone(), action.clone()));
        let t2 = tokio::task::spawn(observer_message(action.clone(), clients.clone()));
        (t1, t2)
    }
}

async fn event_listener<L, E>(trig: L, event: E) 
    where 
        L: EventListener + Send + Sync,
        E: Event + Send + Sync
{
    loop { 
        let len = trig.count_listener().await;
        if len == 0 {
            time::sleep(Duration::from_millis(5)).await;
            continue;
        }
        trig.listen_all(&event).await; 
    }
}

async fn observer_message<M, S>(provider: M, forwarder: S)
    where 
        M: Messanger + Send + Sync,
        S: Consumer + Send + Sync
{
    loop {
        if let Some(packet) = provider.dequeue_message() {
            println!("routing");
            let to = provider.route(&packet.topic).await;
            println!("[to] {:?}", to);
            let dst = match to {
                None => continue,
                Some(dst) => dst[0].clone()
            };
            // TODO: QoS
            forwarder.pubish(dst, packet).await.unwrap();
        }
        time::sleep(Duration::from_millis(10)).await;
    }
}