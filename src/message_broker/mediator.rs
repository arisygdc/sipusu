use std::{sync::Arc, time::Duration};
use tokio::{select, signal, sync::broadcast::{self, Receiver, Sender}, task::JoinHandle, time};
use crate::{ds::{linked_list::List, trie::Trie}, protocol::v5};
use super::{
    client::{
        client::{Client, ClientID, UpdateClient}, 
        clients::Clients
    }, messenger::{
        distributor::MessageDistributor, 
        DistMsgStrategy
    }, provider::EventHandler, ClientReqHandle, EventListener, Forwarder, Message, Messanger
};

pub struct BrokerMediator {
    clients: Clients,
    message_queue: Arc<List<Message>>,
    router: Arc<Trie<ClientID>>,
}

impl BrokerMediator {
    pub async fn new() -> Self {
        let clients = Clients::new().await;
        let message_queue = Arc::new(List::new());
        let router = Arc::new(Trie::new());
        Self{ clients, message_queue, router }
    }
}

impl<'cp> BrokerMediator {
    pub async fn register<CB, R>(&self, new_cl: Client, callback: CB) -> Result<R, String>
    where CB: FnOnce(&'cp mut Client) -> R
    {
        let clid = new_cl.clid.clone();
        println!("[register] client {:?}", clid);
        self.clients.insert(new_cl).await?;
        let result = self.clients.search_mut_client(&clid, callback)
            .await
            .ok_or(format!("cannot inserting client {}", clid))?;
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

        let msg_dist = MessageDistributor::new();
        
        let (tx_sigkill, rx_sigkill) = broadcast::channel::<bool>(1);
        let t1 = tokio::task::spawn(event_listener(
            clients.clone(), 
            action.clone(), 
            rx_sigkill
        ));

        let t2 = tokio::task::spawn(observer_message(
            action.clone(), 
            clients.clone(), 
            msg_dist,
            tx_sigkill
        ));
        (t1, t2)
    }
}

async fn event_listener<L, E>(trig: L, event: E, mut rx_sigkill: Receiver<bool>) 
    where 
        L: EventListener + Send + Sync + 'static,
        E: ClientReqHandle + Send + Sync
{
    println!("[listener] start");
    'listener: loop {
        select! {
            sig = rx_sigkill.recv() => {
                println!("signal kill {:?}", sig);
                println!("trying to shutdown");
                break 'listener;
            },
            len = trig.count_listener() => {
                if len == 0 {
                    time::sleep(Duration::from_millis(5)).await;
                    continue 'listener;
                }
                trig.listen_all(&event).await; 
            }
        }
    }
    println!("[listener] shutdown");
}

async fn observer_message<M, S, D>(
    provider: M, 
    forwarder: S,
    dist: D,
    tx_sigkill: Sender<bool>
) where 
    S: Forwarder + Send + Sync + Clone + 'static,
    D: DistMsgStrategy<S> + Send + Sync,
    M: Messanger + Send + Sync,
{
    println!("[observer] start");
    'observer: loop {
        select! {
            _ = signal::ctrl_c() => {
                println!("send shutdown signal");
                tx_sigkill.send(true).unwrap();
                break 'observer;
            },
            msg = async {
                if let Some(msg) = provider.dequeue_message() {
                    println!("routing");

                    let to = provider.route(&msg.packet.topic).await;
                    println!("[to] {:?}", to);
                    let dst = match to {
                        None => return Err(format!("no subscriber for topic {}", &msg.packet.topic)),
                        Some(dst) => dst
                    };
                    return Ok((msg, dst));
                }
                time::sleep(Duration::from_millis(10)).await;
                Err(String::from("no message"))
            } => {
                let msg = match msg {
                    Ok(v) => v,
                    Err(_) => continue 'observer
                };

                let cl = forwarder.clone();
                let (msg, subs) = msg;
                let packet = msg.packet;
                let publisher = msg.publisher;

                println!("sending message");
                if let v5::ServiceLevel::QoS0 = &packet.qos {
                    let _ = dist.qos0(cl, subs, packet);
                    continue 'observer
                }

                if let Some(p) = publisher {
                    let _ = match &packet.qos {
                        v5::ServiceLevel::QoS1 => { dist.qos1(cl, p, subs, packet) },
                        v5::ServiceLevel::QoS2 => { dist.qos2(cl, p, subs, packet)},
                        _ => continue 'observer 
                    };
                }  
            }
        }
    }
    println!("[observer] shutdown");
}