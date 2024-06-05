mod server;
mod connection;
mod authentication;
mod message_broker;
mod protocol;

use message_broker::{BrokerMediator};
use std::io;
use connection::handler::{Proxy, SecuredStream};
use server::Server;
use tokio::{join, net::ToSocketAddrs, runtime, task::JoinHandle};

fn main() {
    let build_rt = runtime::Builder::new_multi_thread()
        .enable_all()
        .build();

    let rt = match build_rt {
        Ok(v) => v,
        Err(e) => panic!("[runtime] error: {}", e.to_string())
    };
    
    rt.block_on(app())
}

async fn app() {
    let addr = "127.0.0.1:3306".to_owned();
    let joinhandle = bind(addr.clone()).await;

    println!("[server] running on {}", addr);

    let _ = join!(joinhandle);
}

async fn bind(addr: impl ToSocketAddrs + Send + Sync + 'static) -> JoinHandle<io::Result<()>> {
    // let cert = CertificatePath::default();
    println!("running mediator");

    let mediator = BrokerMediator::<SecuredStream>::new();
    
    let handler = Proxy::new(mediator.0).await.unwrap();
    let server = Server::new(None, handler).await;

    let rtask1 = server.bind(addr);
    let task1 = match rtask1 {
        Ok(v) => v,
        Err(e) => panic!("{}", e.to_string())
    };
    task1
}