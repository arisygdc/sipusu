mod server;
mod connection;
mod authentication;
mod core;

use std::io;
use connection::{handler::Proxy, online::Onlines};
use server::{CertificatePath, Server};
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

    let _ = join!(joinhandle.0, joinhandle.1);
}

async fn bind(addr: impl ToSocketAddrs + Send + Sync + 'static) -> (JoinHandle<io::Result<()>>, JoinHandle<()>) {
    let (active_connection, connection_sender) = Onlines::new();
    let handler = Proxy::new(connection_sender).await.unwrap();
    let cert = CertificatePath::default();
    let server = Server::new(handler, Some(cert));

    let rtask1 = server.bind(addr);
    let task1 = match rtask1 {
        Ok(v) => v,
        Err(e) => panic!("{}", e.to_string())
    };

    let task2 = active_connection.stream_connection().await;

    (task1, task2)
}