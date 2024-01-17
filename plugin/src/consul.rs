use futures::lock::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;

use crossbeam::sync::WaitGroup;
use rs_consul::{Config, Consul, RegisterEntityPayload, RegisterEntityService};
use tokio_context::context::Context;

use crate::{async_trait, ServiceContent};
use crate::{Plugin, Synchronize};

#[derive(Debug, Clone)]
pub struct ConsulPlugin {
    cache: Arc<Mutex<HashMap<String, ServiceContent>>>,
    client: Arc<Consul>,
}

impl ConsulPlugin {
    pub(super) async fn new() -> Self {
        dotenv::dotenv().ok();
        // consul://http://localhost:8500
        let uri = std::env::var("REGISTER_ADDR").expect("REGISTER_ADDR is not set");

        let (method, host, port) = Self::validation_parse_uri(&uri);
        let config = Config {
            address: format!("{}://{}:{}", method, host, port),
            ..Default::default()
        };

        ConsulPlugin {
            cache: Arc::new(Mutex::new(HashMap::new())),
            client: Arc::new(Consul::new(config)),
        }
    }

    fn validation_parse_uri(uri: &str) -> (String, String, u16) {
        if !uri.starts_with("consul://") {
            panic!("REGISTER_ADDR must start with consul://");
        }
        if let Ok(issue_list_url) = Url::parse(&uri["consul://".len()..]) {
            if let Some(host) = issue_list_url.host() {
                if let Some(port) = issue_list_url.port() {
                    return (issue_list_url.scheme().to_string(), host.to_string(), port);
                }
            }
        }

        panic!("REGISTER_ADDR is not valid");
    }
}

#[async_trait]
impl Plugin for ConsulPlugin {
    async fn register_service(&self, key: &str, sc: ServiceContent) -> anyhow::Result<()> {
        let entity = RegisterEntityPayload {
            ID: None,
            Node: sc.addr.clone(),
            Address: sc.addr.to_string(),
            Datacenter: None,
            TaggedAddresses: Default::default(),
            NodeMeta: Default::default(),
            Service: Some(RegisterEntityService {
                ID: None,
                Service: sc.service.clone(),
                Tags: vec![key.to_string(), sc.lba],
                TaggedAddresses: Default::default(),
                Meta: Default::default(),
                Port: Some(0),
                Namespace: None,
            }),
            Check: None,
            SkipNodeUpdate: None,
        };

        Ok(self.client.register_entity(&entity).await?)
    }

    async fn get_web_service(&self, _key: &str) -> anyhow::Result<Vec<ServiceContent>> {
        todo!("ConsulPlugin::get_web_service")
    }

    async fn get_backend_service(&self, _key: &str) -> anyhow::Result<(String, Vec<String>)> {
        todo!("ConsulPlugin::get_backend_service")
    }
}

#[async_trait]
impl Synchronize for ConsulPlugin {
    async fn gateway_service_handle(&mut self) {
        todo!()
    }
    async fn backend_service_handle(&mut self, _ctx: Context, _wg: WaitGroup) {
        todo!()
    }
    async fn web_service_handle(&mut self, _ctx: Context, _wg: WaitGroup) {
        todo!()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_parse_uri() {
        let uri = "consul://https://localhost:8500";
        let (method, host, port) = super::ConsulPlugin::validation_parse_uri(uri);
        assert_eq!(method, "https");
        assert_eq!(host, "localhost");
        assert_eq!(port, 8500);
    }
}
