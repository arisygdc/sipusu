use std::{sync::{atomic::Ordering, Arc}, time::Duration};
use bytes::{BufMut, BytesMut};
use tokio::{io, select, signal, sync::Mutex, task::JoinHandle};
use crate::{
    connection::SocketWriter, ds::{
        trie::Trie, GetFromQueue, InsertQueue 
    }, helper::time::sys_now, 
    message_broker::client::storage::{EventType, WALL}, 
    protocol::{
        mqtt::{ClientPacketV5, PING_RES}, 
        v5::{
            self,
            publish::PublishPacket, 
            subsack::SubsAck, 
            subscribe::SubscribePacket, 
            ServiceLevel
        }
    }
};
use crate::connection::SocketReader;
use super::{
    cleanup::Cleanup, client::{
        client::{Client, UpdateClient}, 
        clients::{AtomicClient, Clients}, 
        clobj::{ClientID, ClientSocket}, 
        SessionController
    }, message::{Message, Queue}, 
    router::{SubscriberInstance, TopicRouter}, 
    SendStrategy
};

pub type RouterTree = Arc<Trie<SubscriberInstance>>;

pub struct BrokerMediator {
    clients: Clients,
    tasks: Tasks,
    message_queue: Queue,
    router: RouterTree,
}

impl BrokerMediator {
    pub async fn new() -> Self {
        let clients = Clients::new().await;
        let message_queue = Queue::new();
        let router = Arc::new(Trie::new());
        let tasks = Tasks::new();
        Self{ clients, message_queue, tasks, router }
    }
}

impl<'cp> BrokerMediator {
    pub async fn register<CB, R>(&self, new_cl: Client, callback: CB) -> Result<R, String>
    where CB: FnOnce(&'cp mut ClientSocket) -> R
    {
        let clid = new_cl.clid.clone();
        println!("[register] client {:?}", clid);
        self.clients.insert(new_cl).await?;

        let client = unsafe{self.clients.get_client(&clid)}.await.unwrap();
        let ret = callback(unsafe {
            &mut (*client.load(Ordering::Acquire)).socket
        });

        self.tasks.spawn(
            client, 
            self.message_queue.clone(), 
            self.router.clone()
        ).await;
        Ok(ret)
    }

    pub async fn try_restore_session<CB, R>(&self, clid: ClientID, bucket: &mut UpdateClient, callback: CB) -> io::Result<R> 
    where CB: FnOnce(&'cp mut ClientSocket) -> R
    {
        let restored_client = Client::restore(clid.clone(), bucket).await?;
        self.clients.insert(restored_client).await.unwrap();
        let client = unsafe{self.clients.get_client(&clid).await};
        let client = client.ok_or(io::Error::new(io::ErrorKind::Other, "unknown error"))?;
        let ret = callback(unsafe {
            &mut (*client.load(Ordering::Acquire)).socket
        });

        let client = unsafe{self.clients.get_client(&clid)}.await.unwrap();
        self.tasks.spawn(
            client, 
            self.message_queue.clone(), 
            self.router.clone()
        ).await;
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
            self.tasks.clone(),
            self.router.clone(),
            self.message_queue.clone(),
            clients.clone(),
        ))
    }
}

#[derive(Clone)]
struct Tasks{
    t: Arc<Mutex<Vec<JoinHandle<()>>>>
}

impl Tasks {
    fn new() -> Self {
        Self { t: Arc::new(Mutex::new(Vec::new())) }
    }

    async fn spawn<IQ, RO>(
        &self,
        client: AtomicClient, 
        msg_queue: IQ, 
        router: RO
    ) where 
        IQ: InsertQueue<Message> + Send + Sync + 'static,
        RO: TopicRouter + Send + Sync + 'static
    {
        let mut t = self.t.lock().await;
        t.push(tokio::spawn(spawn_client(
            client, 
            msg_queue, 
            router
        )));
    }
}

impl Cleanup for Tasks {
    async fn clear(self) {
        self.t.lock().await.iter()
        .for_each(|v| {
            v.abort() 
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}


async fn spawn_client<IQ, RO>(
    client: AtomicClient, 
    msg_queue: IQ, 
    router: RO
) where 
    IQ: InsertQueue<Message> + Send + Sync + 'static,
    RO: TopicRouter + Send + Sync + 'static
{
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

        let dur = Duration::from_secs(client.ttl() - t);
        
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

    println!("[Client] {} despawn", unsafe{&mut (*client.load(std::sync::atomic::Ordering::Relaxed))}.clid);
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
    let res = router.subscribe(&client.clid, &sub_packet.list);
    
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

async fn observer<RO, DM, F, S> (
    spawner: S,
    router: RO, 
    msg_queue: DM,
    forwarder: F,
) where 
    S: Cleanup,
    RO: TopicRouter + Send + Sync + 'static,
    DM: GetFromQueue<Message> + Send + Sync + Cleanup + 'static,
    F: SendStrategy + Send + Sync + Clone + Cleanup + 'static,
{
    println!("[observer] start");
    'observer: loop {
        select! {
            _ = signal::ctrl_c() => {
                spawner.clear().await;
                forwarder.clear().await;
                msg_queue.clear().await;
                break 'observer;
            },
            msg = msg_queue.dequeue() => {
                let msg = match msg {
                    Ok(v) => v,
                    Err(_) => continue 'observer
                };
                println!("{:?}", msg.packet);
                let subs = match router.route(&msg.packet.topic) {
                    None => {
                        println!("no subscriber");
                        continue 'observer
                    },
                    Some(s) => s
                };
                
                let fwd = forwarder.clone();
                let order = Publish{
                    msg, subs
                };

                tokio::spawn(order.forward(fwd));
            }
        }
    }
    println!("[observer] shutdown");
}

struct Publish {
    msg: Message, 
    subs: SubscriberInstance
}

impl Publish {
    async fn forward<F>(self, forwarder: F)
    where 
        F: SendStrategy + Send + Sync + Clone + 'static,
    {
        let publisher_id = self.msg.publisher;
        let packet = self.msg.packet;
        let subs = self.subs;

        // Downgrade qos by max qos
        let qos = packet.qos.code().min(subs.max_qos.code());
        let qos = ServiceLevel::try_from(qos)
            .unwrap_or_default();

        if let v5::ServiceLevel::QoS0 = &qos {
            let buffer = packet.encode().unwrap();
            forwarder.qos0(&subs.clid, &buffer).await;
            return ;
        }

        let publisher_id = publisher_id.unwrap();
        let packet_id = packet.packet_id.unwrap();
        let buffer = packet.encode().unwrap();

        let _res = match qos {
            ServiceLevel::QoS1 => forwarder.qos1(&publisher_id, &subs.clid, packet_id, &buffer).await,
            ServiceLevel::QoS2 => forwarder.qos2(&publisher_id, &subs.clid, packet_id, &buffer).await,
            _ => {return ;}
        };
    }
}