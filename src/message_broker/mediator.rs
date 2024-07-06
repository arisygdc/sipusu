use std::{sync::{atomic::{AtomicU64, Ordering}, Arc}, time::Duration};
use bytes::{BufMut, BytesMut};
use tokio::{io, select, signal, task::JoinHandle, time};
use crate::{
    connection::SocketWriter, ds::{
        linked_list::List, trie::Trie, GetFromQueue, InsertQueue 
    }, helper::time::sys_now, message_broker::client::{storage::{EventType, WALL}, SAFETY_OFFTIME}, protocol::{
        mqtt::{ClientPacketV5, PING_RES}, 
        v5::{
            self,
            malform::Malformed, publish::PublishPacket, 
            subsack::{SubAckResult, SubsAck}, 
            subscribe::{Subscribe, SubscribePacket}, 
            ServiceLevel
        }
    }
};
use crate::connection::SocketReader;
use super::{
    cleanup::Cleanup, client::{
        client::{Client, UpdateClient}, clients::{AtomicClient, Clients}, clobj::{ClientID, ClientSocket}, SessionController
    }, message::{Message, MessageQueue}, SendStrategy
};

pub type RouterTree = Arc<Trie<SubscriberInstance>>;

pub struct BrokerMediator {
    clients: Clients,
    message_queue: MessageQueue,
    spawn_counter: Arc<AtomicU64>,
    router: RouterTree,
}

impl BrokerMediator {
    pub async fn new() -> Self {
        let clients = Clients::new().await;
        let message_queue = Arc::new(List::new());
        let router = Arc::new(Trie::new());
        let spawn_counter =  Arc::new(AtomicU64::default());
        Self{ clients, message_queue, spawn_counter, router }
    }
}

impl<'cp> BrokerMediator {
    pub async fn register<CB, R>(&self, new_cl: Client, callback: CB) -> Result<R, String>
    where CB: FnOnce(&'cp mut ClientSocket) -> R
    {
        let clid = new_cl.clid.clone();
        println!("[register] client {:?}", clid);
        self.clients.insert(new_cl).await?;
        let result = self.clients.search_mut_client(&clid, |c| callback(&mut c.socket))
            .await
            .ok_or(format!("cannot inserting client {}", clid))?;

        let client = unsafe{self.clients.get_client(&clid)}.await.unwrap();
        let queue = self.message_queue.clone();
        let router = self.router.clone();
        spawn_client(client, self.spawn_counter.clone(), queue, router);
        Ok(result)
    }

    pub async fn try_restore_session<CB, R>(&self, clid: ClientID, bucket: &mut UpdateClient, callback: CB) -> io::Result<R> 
    where CB: FnOnce(&'cp mut ClientSocket) -> R
    {
        let client = Client::restore(clid.clone(), bucket).await?;
        self.clients.insert(client).await.unwrap();
        let client = unsafe{self.clients.get_client(&clid).await};
        let client = client.ok_or(io::Error::new(io::ErrorKind::Other, "unknown error"))?;
        let ret = callback(unsafe {
            &mut (*client.load(Ordering::Acquire)).socket
        });
        let queue = self.message_queue.clone();
        let router = self.router.clone();
        spawn_client(client, self.spawn_counter.clone(), queue, router);
        Ok(ret)
    }

    pub async fn is_still_alive(&self, clid: &ClientID) -> Option<bool> {
        let t = sys_now();
        self.clients.search_mut_client(clid, |c| {
            c.is_alive(t)
        })
        .await
    }

    pub fn join_handle(&self) -> JoinHandle<()> {
        let clients = self.clients.clone();
        
        tokio::task::spawn(observer(
            self.router.clone(),
            self.message_queue.clone(),
            clients.clone(),
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
            let instance = SubscriberInstance {
                clid: clid.clone(),
                max_qos: sub.max_qos.clone()
            };

            self.insert(&sub.topic, instance).await;
            res.push(Ok(sub.max_qos.clone()));
        }
        Ok(res)
    }

    async fn route(&self, topic: &str) -> Option<Vec<SubscriberInstance>> {
        self.get(topic).await
    }
}

fn spawn_client<IQ, RO>(client: AtomicClient, spawn_counter: Arc<AtomicU64>, msg_queue: IQ, router: RO) 
    where 
        IQ: InsertQueue<Message> + Send + Sync + 'static,
        RO: TopicRouter + Send + Sync + 'static
{
    tokio::spawn(async move {
        spawn_counter.fetch_add(1, Ordering::AcqRel);
        let mut buffer = BytesMut::zeroed(1024);
        println!("[Client] {} spawned", unsafe{&mut (*client.load(std::sync::atomic::Ordering::Relaxed))}.clid);
        'lis: loop {
            let client = unsafe {&mut *client.load(std::sync::atomic::Ordering::Relaxed)};

            let t = sys_now();
            if !client.is_alive(t) {
                let log = client.storage.clone();

                let sevent = WALL{
                    time: t+1, 
                    value: EventType::DisconnectByServer(client.expiration_time())
                };

                log.log_session(&[sevent])
                .await
                .unwrap();

                println!("[Client] {} dead", client.clid);
                break 'lis;
            }

            let dur = Duration::from_secs(SAFETY_OFFTIME);
            
            let readed = match client.socket.read_timeout(&mut buffer, dur).await {
                Ok(readed) => readed,
                Err(err) => {
                    if err.kind() == io::ErrorKind::TimedOut {
                        continue 'lis;
                    }

                    let log = client.storage.clone();
                    let sevent = WALL{
                        time: t+1, 
                        value: EventType::ClientDisconnected(client.expiration_time())
                    };

                    log.log_session(&[sevent])
                        .await
                        .unwrap();

                    break 'lis;
                }
            };
            
            if readed == 0 {
                client.kill();
                continue 'lis;
            }

            let mut packet_buf = buffer.split_to(readed);
            let incoming_packet = ClientPacketV5::decode(&mut packet_buf);
            let packet_received = incoming_packet.unwrap();
            match client.keep_alive(t+1) {
                Ok(_) => {},
                Err(_) => continue
            };

            match packet_received {
                ClientPacketV5::PingReq => { let _ = client.socket.write_all(&PING_RES).await; },
                ClientPacketV5::Publish(pub_packet) => queue_message(&msg_queue, &client.clid, pub_packet),
                ClientPacketV5::Subscribe(sub_packet) => subscribe_topics(&router, client, sub_packet).await
            };
            
            buffer.reserve(readed);
            buffer.put_bytes(0, readed);
        }

        spawn_counter.fetch_sub(1, Ordering::AcqRel);
        println!("[Client] {} despawn", unsafe{&mut (*client.load(std::sync::atomic::Ordering::Relaxed))}.clid);
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
            println!("Malformed");
            unimplemented!()
        }
    };

    let response = SubsAck{
        id: sub_packet.id,
        properties: None,
        return_codes: recode
    };

    let buffer = response.encode().unwrap();
    let save = client.storage.clone();
    let save = save.subscribe(&sub_packet.list);
    let net = client.socket.write_all(&buffer);
    let (save, net) = tokio::join!(net, save);
    net.unwrap();
    save.unwrap();
}

async fn observer<RO, DM, F> (
    router: RO, 
    msg_queue: DM,
    forwarder: F,
) where 
    RO: TopicRouter + Send + Sync + 'static,
    DM: GetFromQueue<Message> + Send + Sync + Cleanup + 'static,
    F: SendStrategy + Send + Sync + Clone + Cleanup + 'static,
{
    println!("[observer] start");
    'observer: loop {
        select! {
            _ = signal::ctrl_c() => {
                forwarder.clear().await;
                msg_queue.clear().await;
                break 'observer;
            },
            msg = wait_message(&msg_queue, &router) => {
                let msg = match msg {
                    Ok(v) => v,
                    Err(_) => continue 'observer
                };
                
                let (msg, subs) = msg;
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
            forwarder.qos0(&ins.clid, &buffer).await;
        }
        return ;
    }

    let publisher_id = publisher_id.unwrap();
    let packet_id = packet.packet_id.unwrap();
    let buffer = packet.encode().unwrap();
    
    // Downgrade qos by max qos
    for ins in subs {
        let qos = packet.qos.code().min(ins.max_qos.code());
        let qos = ServiceLevel::try_from(qos)
            .unwrap_or_default();

        let _res = match qos {
            ServiceLevel::QoS0 => { 
                forwarder.qos0(&ins.clid, &buffer).await;
                continue;
            }, 
            ServiceLevel::QoS1 => forwarder.qos1(&publisher_id, &ins.clid, packet_id, &buffer).await,
            ServiceLevel::QoS2 => forwarder.qos2(&publisher_id, &ins.clid, packet_id, &buffer).await,
        };
    };
}