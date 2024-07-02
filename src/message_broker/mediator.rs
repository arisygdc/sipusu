use std::{sync::Arc, time::Duration};
use bytes::BytesMut;
use tokio::{select, signal, sync::broadcast::{self, Sender}, task::JoinHandle, time};
use crate::{
    connection::SocketWriter, 
    ds::{
        linked_list::List, trie::Trie, GetFromQueue, InsertQueue 
    }, 
    helper::time::sys_now, 
    protocol::{
        mqtt::{ClientPacketV5, PING_RES}, 
        v5::{self, malform::Malformed, publish::PublishPacket, subsack::{SubAckResult, SubsAck}, subscribe::{Subscribe, SubscribePacket}, ServiceLevel}
    }
};
use crate::connection::SocketReader;
use super::{
    client::{
        client::{Client, ClientID, UpdateClient}, 
        clients::{AtomicClient, Clients}, SessionController
    }, Message, SendStrategy
};

pub type RouterTree = Arc<Trie<SubscriberInstance>>;

pub struct BrokerMediator {
    clients: Clients,
    message_queue: Arc<List<Message>>,
    router: RouterTree,
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

        let client = unsafe{self.clients.get_client(&clid)}.await.unwrap();
        let queue = self.message_queue.clone();
        let router = self.router.clone();
        spawn_client(client, queue, router);
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

    pub fn join_handle(&mut self) -> JoinHandle<()> {
        let clients = self.clients.clone();
        
        let (tx_sigkill, rx_sigkill) = broadcast::channel::<bool>(1);
        // let router = self.router.clone();
        tokio::task::spawn(observer_message(
            self.router.clone(),
            self.message_queue.clone(),
            clients.clone(),
            tx_sigkill
        ))
    }
}

#[derive(Clone)]
pub struct SubscriberInstance {
    pub clid: ClientID,
    pub max_qos: ServiceLevel
}

impl PartialEq for SubscriberInstance {
    fn eq(&self, other: &Self) -> bool {
        self.clid.eq(&other.clid)
    }

    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}

pub trait TopicRouter {
    fn subscribe(&self, clid: &ClientID, subs: &[Subscribe]) -> impl std::future::Future<Output = Result<Vec<SubAckResult>, Malformed>> + Send;
    fn route(&self, topic: &str) -> impl std::future::Future<Output = Option<Vec<SubscriberInstance>>> + Send;
}

impl TopicRouter for Arc<Trie<SubscriberInstance>> {
    async fn subscribe(&self, clid: &ClientID, subs: &[Subscribe]) -> Result<Vec<SubAckResult>, Malformed> {
        let mut res = Vec::with_capacity(subs.len());
        for sub in subs {
            let max_qos: ServiceLevel = sub.max_qos.try_into()?;
            let instance = SubscriberInstance {
                clid: clid.clone(),
                max_qos: max_qos.clone()
            };

            self.insert(&sub.topic, instance).await;
            res.push(Ok(max_qos));
        }
        Ok(res)
    }

    async fn route(&self, topic: &str) -> Option<Vec<SubscriberInstance>> {
        self.get(topic).await
    }
}

impl<Message: std::default::Default> InsertQueue<Message> for Arc<List<Message>> 
{
    fn enqueue(&self, val: Message) {
        self.append(val)
    }
}

impl<Message: std::default::Default> GetFromQueue<Message> for Arc<List<Message>> {
    fn dequeue(&self) -> Option<Message> {
        self.take_first()
    }
}

fn spawn_client<IQ, RO>(client: AtomicClient, msg_queue: IQ, router: RO) 
    where 
        IQ: InsertQueue<Message> + Send + Sync + 'static,
        RO: TopicRouter + Send + Sync + 'static
{
    tokio::spawn(async move {
        let mut buffer = BytesMut::zeroed(1024);

        'lis: loop {
            let client = unsafe{&mut (*client.load(std::sync::atomic::Ordering::Relaxed))};
            let t = sys_now();
            if !client.is_alive(t) {
                // Saving state
                break 'lis;
            }

            let dur = Duration::from_secs(2);
            let readed = client.read_timeout(&mut buffer, dur).await.unwrap();
            if readed > 0 {
                client.kill();
                continue 'lis;
            }

            let incoming_packet = ClientPacketV5::decode(&mut buffer);
            let packet_received = incoming_packet.unwrap();
            match client.keep_alive(t+2) {
                Ok(_) => {},
                Err(_) => continue
            };

            match packet_received {
                ClientPacketV5::PingReq => { let _ = client.write_all(&PING_RES).await; },
                ClientPacketV5::Publish(pub_packet) => queue_message(&msg_queue, &client.clid, pub_packet),
                ClientPacketV5::Subscribe(sub_packet) => subscribe_topics(&router, client, sub_packet).await
            };
            buffer.reserve(1024);
        }
    });
}

fn queue_message<IQ>(msg_queue: &IQ, clid: &ClientID, packet: PublishPacket)
where IQ: InsertQueue<Message>
{
    let mut msg = Message {
        packet: packet,
        publisher: None
    };

    if msg.packet.qos.code() > 0 {
        msg.publisher = Some(clid.clone());
    }

    msg_queue.enqueue(msg)
}

async fn subscribe_topics<RO>(router: &RO, client: &mut Client, sub_packet: SubscribePacket) 
where RO: TopicRouter
{
    let res = router.subscribe(&client.clid, &sub_packet.list).await;
    
    let recode = match res {
        Ok(res) => res,
        Err(_err) => {

            unimplemented!()
        }
    };

    let response = SubsAck{
        id: sub_packet.id,
        properties: None,
        return_codes: recode
    };

    let buffer = response.encode().unwrap();
    let _net = client.write_all(&buffer).await;
}

async fn observer_message<RO, DM, F>(
    router: RO, 
    msg_queue: DM,
    forwarder: F,
    tx_sigkill: Sender<bool>
) where 
    RO: TopicRouter + Send + Sync + 'static,
    DM: GetFromQueue<Message> + Send + Sync + 'static,
    F: SendStrategy + Send + Sync + Clone + 'static,
{
    println!("[observer] start");
    'observer: loop {
        select! {
            _ = signal::ctrl_c() => {
                println!("send shutdown signal");
                tx_sigkill.send(true).unwrap();
                break 'observer;
            },
            msg = wait_message(&msg_queue, &router) => {
                let msg = match msg {
                    Ok(v) => v,
                    Err(_) => continue 'observer
                };

                let (msg, subs) = msg;

                println!("sending message");
                publish(forwarder.clone(), msg, subs).await
            }
        }
    }
    println!("[observer] shutdown");
}

async fn wait_message<DM, RO>(msg_queue: &DM, router: &RO) -> Result<(Message, Vec<SubscriberInstance>), String>
where 
    DM: GetFromQueue<Message>,
    RO: TopicRouter,
{
    if let Some(msg) = msg_queue.dequeue() {
        println!("routing");

        let to = router.route(&msg.packet.topic).await;
        let dst = match to {
            None => return Err(format!("no subscriber for topic {}", &msg.packet.topic)),
            Some(dst) => dst
        };
        return Ok((msg, dst));
    }
    time::sleep(Duration::from_millis(10)).await;
    Err(String::from("no message"))
}

pub async fn publish<F>(forwarder: F, msg: Message, subs: Vec<SubscriberInstance>) 
where 
    F: SendStrategy + Send + Sync + Clone + 'static,
{
    let publisher_id = msg.publisher;
    let packet = msg.packet;

    if let v5::ServiceLevel::QoS0 = &packet.qos {
        for ins in subs {
            let buffer = packet.encode().unwrap();
            forwarder.qos0(&ins.clid, &buffer);
        }
        return ;
    }

    let slvl_reduce = match packet.qos {
        v5::ServiceLevel::QoS0 => {
            for ins in subs {
                let buffer = packet.encode().unwrap();
                forwarder.qos0(&ins.clid, &buffer);
            }
            return ;
        }, 
        v5::ServiceLevel::QoS1 => false,
        v5::ServiceLevel::QoS2 => true
    };

    // TODO: Downgrade qos by max qos
    let publisher_id = publisher_id.unwrap();
    let packet_id = packet.packet_id.unwrap();
    let buffer = packet.encode().unwrap();
    for s in subs {
        let _res = match slvl_reduce {
            false => forwarder.qos1(&publisher_id, &s.clid, packet_id, &buffer).await,
            true => forwarder.qos2(&publisher_id, &s.clid, packet_id, &buffer).await,
        };
    };
}