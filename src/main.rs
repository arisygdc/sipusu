mod server;
mod connection;
mod authentication;
mod message_broker;
mod protocol;
mod helper;
mod ds;

use message_broker::mediator::BrokerMediator;
use std::time::Duration;
use connection::handler::Proxy;
use server::Server;
use tokio::{join, runtime};

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
    println!("running mediator");

    let mediator: BrokerMediator = BrokerMediator::new().await;
    let broker_task = mediator.join_handle();
    let handler = Proxy::new(mediator).await.unwrap();
    let server = Server::new(None, handler).await;

    let addr = "127.0.0.1:3306".to_owned();
    let rtask1 = server.bind(addr.clone());
    let svr = match rtask1 {
        Ok(v) => v,
        Err(e) => panic!("{}", e.to_string())
    };

    
    println!("[server] running on {}", addr);
    let _ = join!(svr, broker_task);

    tokio::time::sleep(Duration::from_secs(3)).await;
    println!("[server] shutdown")
}
