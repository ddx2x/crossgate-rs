mod proxy;
pub use proxy::{call, ProxyError, ReverseProxy};

use hyper::client::HttpConnector;

use hyper::Client;

#[inline]
pub fn get_proxy_client() -> &'static ReverseProxy<HttpConnector> {
    &CLIENT
}

use lazy_static::lazy_static;

lazy_static! {
    static ref CLIENT: ReverseProxy<HttpConnector> = ReverseProxy::new(Client::new());
}
