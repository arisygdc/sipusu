use std::sync::Arc;

use connection::{ActiveConnection, Proxy};
use server::{CertificatePath, Server};
use tokio::{join, runtime};
mod server;
mod connection;

fn main() {
    let build_rt = runtime::Builder::new_multi_thread()
        .enable_all()
        .build();

    let rt = match build_rt {
        Ok(v) => v,
        Err(e) => {
            println!("[runtime] error: {}", e.to_string());
            panic!()
        }
    };
    
    rt.block_on(app())
}

async fn app() {
    let active_connection = Arc::new(ActiveConnection::new());
    let handler = Proxy::new(active_connection.clone());
    let cert = CertificatePath::new(String::from("/var/test_host/cert.pem"), String::from("/var/test_host/key.pem"));
    let server = Server::new(handler, Some(cert));

    let bind_addr = "127.0.0.1:3306".to_owned();
    let rthread1 = server.bind(bind_addr.clone());
    let thread1 = match rthread1 {
        Ok(v) => v,
        Err(e) => {
            println!("{}", e.to_string());
            panic!()
        }
    };

    let _ = join!(thread1);
}