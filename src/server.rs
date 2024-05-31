use std::{fs::File, io::{self, BufReader}, path::Path, sync::Arc};
use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio::{net::{TcpListener, TcpStream, ToSocketAddrs}, task::JoinHandle};
use tokio_rustls::{rustls::{pki_types::{CertificateDer, PrivateKeyDer}, ServerConfig}, server::TlsStream, TlsAcceptor};

pub struct Server<H: Handler + Send + Sync + 'static> {
    cert: Option<CertificatePath>,
    handler: Arc<H>
}

impl<H: Handler + Send + Sync + 'static> Server<H> {
    pub fn new(handler: H, cert: Option<CertificatePath>) -> Self {
        let handler = Arc::new(handler);
        Self { cert, handler }
    }

    pub fn bind<A>(
        self,
        addr: A
    ) -> io::Result<JoinHandle<io::Result<()>>> 
    where
        A: ToSocketAddrs + Send + Sync + 'static
    {
        let cert = self.cert.as_ref()
            .ok_or(io::Error::new(
                io::ErrorKind::NotFound, "certificate"
            ))?;

        let certs = cert.load_certs()?;
        let key = cert.load_keys()?;

        let tls_config = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

        let fut = self.bind_secure(addr, tls_config);
        Ok(tokio::spawn(fut))
    }

    async fn bind_secure<A>(
        self,
        addr: A, 
        tls_config: ServerConfig
    ) -> io::Result<()> where A: ToSocketAddrs + Send {
        let acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let listener = TcpListener::bind(&addr).await?;
        
        loop {
            let (stream, _peer_addr) = listener.accept().await?;
            let acceptor = acceptor.clone();
            
            let handle: Arc<H> = self.handler.clone();
            
            let fut = async move {
                let secured_stream = acceptor.accept(stream).await.unwrap();
                handle.process_request(secured_stream).await;
            };

            tokio::task::spawn(fut);
        }
    }
}

pub struct CertificatePath {
    cert: String,
    private_key: String
}

impl CertificatePath {
    pub fn new(cert: String, private_key: String) -> Self {
        CertificatePath { cert, private_key }
    }

    fn load_certs(&self) -> io::Result<Vec<CertificateDer<'static>>> {
        let path = Path::new(&self.cert);
        certs(&mut BufReader::new(File::open(path)?)).collect()
    }

    fn load_keys(&self) -> io::Result<PrivateKeyDer<'static>> {
        let path = Path::new(&self.private_key);
        pkcs8_private_keys(&mut BufReader::new(File::open(path)?))
            .next()
            .unwrap()
            .map(Into::into)
    }
}

pub trait Handler {
    fn process_request(&self, stream: TlsStream<TcpStream>) -> impl std::future::Future<Output = ()> + Send;
}