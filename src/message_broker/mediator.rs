use std::{sync::Arc, time::Duration};
use tokio::{task::JoinHandle, time};
use crate::{connection::{line::SocketConnection, ConnectionID}, protocol::mqtt::PublishPacket};
use super::{client::{Client, ClientID}, clients::Clients, linked_list::List, provider::EventHandler, trie::Trie, Consumer, Event, EventListener, Messanger};

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

    /// wakeup session
    /// replacing old socket with incoming socket connection 
    pub async fn try_restore_connection(&self, clid: &ClientID, bucket: &mut Option<SocketConnection>) -> Result<(), String> {
        let res = match self.clients.search_mut_client(clid, |c| c.restore_connection(bucket))
            .await {
                None => return Err(String::from("Not found")),
                Some(res) => res
            };
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