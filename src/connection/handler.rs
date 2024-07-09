use std::{io, sync::atomic::AtomicU32};
use super::{errors::ConnError, handshake::{MqttConnectRequest, MqttConnectedResponse}, line::SocketConnection, ConnectionID};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use crate::{
    message_broker::{
        client::{client::{Client, UpdateClient}, 
        clobj::{ClientID, Limiter}}, 
        mediator::BrokerMediator, MAX_QOS
    }, protocol::v5::{
        connack::{ConnackPacket, Properties}, 
        connect::ConnectPacket
    }, server::Wire
};

#[allow(dead_code)]
pub struct Proxy {
    broker: BrokerMediator,
    access_total: AtomicU32,
}

impl Proxy {
    pub async fn new(broker: BrokerMediator) -> io::Result<Self> {
        let access_total = AtomicU32::new(1);
        Ok(Self { access_total, broker })
    }
    
    async fn establish_connection(&self, connid: ConnectionID, mut conn: SocketConnection) -> Result<(), ConnError> {
        let req_ack = conn.read_request().await?;
        let mut connack_packet = ConnackPacket::default();

        let srv_var = collect(req_ack, &mut connack_packet).unwrap();
        self.start_session(
            connack_packet,
            connid,
            conn,
            srv_var
        ).await.unwrap();
        Ok(())
    }

    // TODO: properties
    async fn start_session(
        &self, 
        mut response: ConnackPacket,
        connid: ConnectionID,
        conn: SocketConnection,
        srv_var: ServerVariable
    ) -> Result<(), String> {
        let mut conn = conn;
        let session = self.broker.is_still_alive(&srv_var.clid).await;
        if let Some(still_alive) = session {
            if still_alive {
                // TODO: send disconnect with response code
                return Err("()".to_string());
            }
        }

        if !srv_var.clean_start {
            println!("try restoring connection");
            let mut bucket = UpdateClient {
                conid: Some(connid.clone()),
                socket: Some(conn)
            };

            let restore_feedback = 
            self.broker.try_restore_session(srv_var.clid.clone(), &mut bucket, |s| {
                response.session_present = true;
                s.connack(&response)
            }).await;
            
            match restore_feedback {
                Ok(fb) => {
                    fb.await.unwrap();
                    println!("client restored");
                    return  Ok(());
                }, Err(err) => println!("{}", err.to_string())
            }
            
            
            println!("failed to restore");
            conn = bucket.socket
                .take()
                .unwrap();
        }

        let mut limit = Limiter::default();
        if let Some(ref v) = response.properties {
            limit = Limiter::new(
                v.receive_maximum, 
                v.maximum_packet_size,
                v.topic_alias_maximum
            );
        }

        let client = Client::new(
            connid, 
            conn, 
            srv_var.clid.clone(), 
            srv_var.keep_alive,
            srv_var.expr_interval,
            srv_var.protocol_level,
            limit
        ).await;

        let cb = self.broker.register(client, |s| async {
            s.connack(&response).await
        }).await.unwrap();
        cb.await.unwrap();
        Ok(())
    }

    fn request_id(&self) -> ConnectionID {
        let fetch = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Release);
        ConnectionID(fetch)
    }
}

impl Wire for Proxy {
    async fn connect_with_tls(&self, stream: TcpStream, tls: TlsAcceptor) {
        let id = self.request_id();
        println!("[stream] process id {}", id);
        let stream = match tls.accept(stream).await {
            Ok(v) => v,
            // TODO: Specify the error and response error message
            Err(err) => {
                eprintln!("[tls] conn {}, error: {}", id, err.to_string());
                return ;
            }
        };
        
        println!("[stream] secured");
        let stream = SocketConnection::Secure(stream);
        let _ = self.establish_connection(id, stream).await;
    }

    async fn connect(&self, stream: TcpStream) {
        let id = self.request_id();
        println!("[stream] process id {}", id);
        let stream = stream;
        let stream = SocketConnection::Plain(stream);
        let _ = self.establish_connection(id, stream).await;
    }
}

struct ServerVariable {
    clid: ClientID,
    clean_start: bool,
    keep_alive: u16,
    protocol_level: u8,
    expr_interval: u32,
}

// TODO: on notes
fn collect(req: ConnectPacket, res: &mut ConnackPacket) -> Result<ServerVariable, String> {
    let clean_start = req.clean_start();

    let is_generate_clid = req.client_id.len() == 0;
    let clid = match is_generate_clid {
        true => ClientID::new("raw_clid".to_string()),
        false => ClientID::new(req.client_id)
    };
    
    let mut srv_var = ServerVariable {
        clid: clid.clone(),
        clean_start,
        keep_alive: req.keep_alive,
        protocol_level: req.protocol_level,
        expr_interval: 0,
    };

    let req_prop = match req.properties {
        Some(prop) => prop,
        None => return Ok(srv_var)
    };
    
    srv_var.expr_interval = req_prop
        .session_expiry_interval
        .unwrap_or_default();

    let mut res_prop = Properties::default();
    res_prop.session_expiry_interval = Some(srv_var.expr_interval); 
    if is_generate_clid {
        res_prop.assigned_client_identifier = Some(clid.to_string())
    }

    res_prop.receive_maximum = req_prop.receive_maximum;
    res_prop.maximum_qos = Some(MAX_QOS);
    // res_prop.retain_available
    res_prop.maximum_packet_size = req_prop.maximum_packet_size;
    res_prop.topic_alias_maximum = req_prop.topic_alias_maximum;
    // res_prop.reason_string
    res_prop.user_properties = req_prop.user_properties;
    // res_prop.wildcard_subscription_available
    // res_prop.subscription_identifier_available
    // res_prop.shared_subscription_available
    res_prop.server_keep_alive = Some(req.keep_alive);
    // res_prop.response_information
    // res_prop.server_reference
    // res_prop.authentication_method
    // res_prop.authentication_data
    res.properties = Some(res_prop);
    Ok(srv_var)
}