use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{task::JoinHandle, time};
use crate::{connection::ConnectionID, protocol::mqtt::PublishPacket};
use super::{client::{Client, Clients}, linked_list::List, provider::EventHandler, trie::Trie, Consumer, Event, EventListener, Messanger};

pub struct BrokerMediator {
    clients: Clients,
    message_queue: Arc<List<PublishPacket>>,
    router: Arc<Trie<ConnectionID>>,
}

impl BrokerMediator {
    pub fn new() -> Self {
        let clients = Clients::new();
        let message_queue = Arc::new(List::new());
        let router = Arc::new(Trie::new());
        Self{ clients, message_queue, router }
    }
}

impl BrokerMediator {
    pub async fn register(&self, client: Client) {
        let client = client;
        println!("[register] client {:?}", client.conid);
        self.clients.insert(client).await
    }

    // FIXME: wakeup connection
    // - on the same ip can be different port
    pub async fn wakeup_exists(&self, clid: &str, addr: &SocketAddr) -> Option<ConnectionID> {
        let res = self.clients.find(clid, |c| {
            if c.addr.eq(addr) {
                c.set_alive(true);
                return Some(c.conid.clone());
            }
                
            None
        }).await?;
        res
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