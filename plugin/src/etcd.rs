use std::{collections::HashMap, sync::Arc};

use crate::{async_trait, Plugin, ServiceContent, Synchronize};
use crossbeam::sync::WaitGroup;
use etcd_client::{Client, GetOptions, PutOptions, WatchOptions};
use futures::lock::Mutex;
use tokio_context::context::Context;

pub(super) const LEASE: i64 = 3;
pub(super) const WEB_SERVICE: &str = "/web/service";
pub(super) const BACKEND_SERVICE: &str = "/backend/service";

#[derive(Clone)]
pub struct EtcdPlugin {
    inner: Arc<Mutex<HashMap<String, ServiceContent>>>,
    cache: Arc<Mutex<HashMap<String, Vec<ServiceContent>>>>,
    client: Client,
}

impl EtcdPlugin {
    pub(super) async fn new() -> Self {
        dotenv::dotenv().ok();
        // etcd://http://node1:2379,http://node2:2379
        let uri = std::env::var("REGISTER_ADDR").expect("REGISTER_ADDR is not set");

        let endpoints = Self::validation_parse_uri(&uri);
        let client = Client::connect(endpoints, None)
            .await
            .expect("etcd connect failed");

        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            cache: Arc::new(Mutex::new(HashMap::new())),
            client,
        }
    }

    fn validation_parse_uri(uri: &str) -> Vec<String> {
        if !uri.starts_with("etcd://") {
            panic!("REGISTER_ADDR must start with etcd://");
        }
        return uri["etcd://".len()..]
            .split(",")
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
    }

    async fn register(&self, key: &str, sc: &ServiceContent) -> anyhow::Result<()> {
        let mut service: String = "".into();

        if sc.r#type == 1 {
            service = format!("{}{}", WEB_SERVICE, key);
        } else if sc.r#type == 2 {
            service = format!("{}{}", BACKEND_SERVICE, key);
        }

        log::debug!("start register service: {}", service.clone());

        match self.client.clone().lease_grant(LEASE, None).await {
            Ok(resp) => {
                if let Ok((lease, _)) = self.client.clone().lease_keep_alive(resp.id()).await {
                    if let Ok(_) = self
                        .client
                        .clone()
                        .put(
                            service.clone(),
                            sc.clone(),
                            Some(PutOptions::new().with_lease(lease.id())),
                        )
                        .await
                    {
                        log::debug!("register service: {} done", service);
                        return Ok(());
                    }
                }

                return Err(anyhow::anyhow!("etcd register failed"));
            }
            Err(e) => {
                return Err(anyhow::anyhow!("etcd register failed: {}", e.to_string()));
            }
        }
    }

    async fn unregister(&self) -> anyhow::Result<()> {
        let inner = self.inner.lock().await;

        for (key, sc) in inner.iter() {
            let mut service: String = "".into();
            if sc.r#type == 1 {
                service = format!("{}{}", WEB_SERVICE, key);
            } else if sc.r#type == 2 {
                service = format!("{}{}", BACKEND_SERVICE, key);
            }

            let _ = self.client.clone().delete(service.clone(), None).await;
        }

        Ok(())
    }
}

#[async_trait]
impl Plugin for EtcdPlugin {
    async fn register_service(&self, key: &str, sc: ServiceContent) -> anyhow::Result<()> {
        let key = format!("{}/{}", key, sc.addr);

        let mut cache = self.cache.lock().await;
        cache.insert(key.to_string(), vec![sc.clone()]);
        let mut inner = self.inner.lock().await;
        inner.insert(key.to_string(), sc.clone());

        Ok(self.register(&key, &sc).await?)
    }

    async fn get_web_service(&self, _key: &str) -> anyhow::Result<Vec<ServiceContent>> {
        let key = format!("{}{}", WEB_SERVICE, _key);

        let cache = self.cache.lock().await;
        if let Some(v) = cache.get(&key) {
            return Ok(v
                .iter()
                .map(|item| item.clone())
                .collect::<Vec<ServiceContent>>());
        }

        if let Ok(resp) = self
            .client
            .clone()
            .get(key, Some(GetOptions::default().with_prefix()))
            .await
        {
            return Ok(resp
                .kvs()
                .iter()
                .map(|kv| {
                    serde_json::from_str::<ServiceContent>(kv.value_str().unwrap_or("{}")).unwrap()
                })
                .collect::<Vec<ServiceContent>>());
        }
        return Err(anyhow::anyhow!("get web service failed"));
    }

    async fn get_backend_service(&self, _key: &str) -> anyhow::Result<(String, Vec<String>)> {
        todo!("EtcdPlugin::get_backend_service")
    }
}

#[async_trait]
impl Synchronize for EtcdPlugin {
    async fn gateway_service_handle(&mut self) {
        let _self = self.clone();

        let block = async move {
            match _self
                .client
                .clone()
                .watch(
                    format!("{}", WEB_SERVICE,),
                    Some(WatchOptions::default().with_prefix()),
                )
                .await
            {
                Ok((_, mut stream)) => {
                    while let Ok(Some(resp)) = stream.message().await {
                        for event in resp.events().iter() {
                            match event.event_type() {
                                etcd_client::EventType::Put => {
                                    let kv = event.kv().unwrap();
                                    let key = kv.key_str().unwrap();
                                    let value = kv.value_str().unwrap();
                                    let mut inner = _self.inner.lock().await;
                                    inner.insert(
                                        key.to_string(),
                                        serde_json::from_str(value).unwrap(),
                                    );
                                }
                                etcd_client::EventType::Delete => {
                                    let kv = event.kv().unwrap();
                                    let key = kv.key_str().unwrap();
                                    let mut inner = _self.inner.lock().await;
                                    inner.remove(key);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    panic!("etcd watch failed: {}", e.to_string());
                }
            }
        };

        tokio::spawn(block);
    }
    async fn backend_service_handle(&mut self, ctx: Context, wg: WaitGroup) {
        let mut ctx = ctx;
        let self_cp0 = self.clone();
        let self_cp1 = self.clone();
        let self_cp2 = self.clone();

        let block = async move {
            // auto register every lease-1s
            let block0 = async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        (LEASE - 1 as i64).try_into().unwrap(),
                    ))
                    .await;

                    log::debug!("auto register");

                    let inner = self_cp0.inner.lock().await;

                    for (key, sc) in inner.iter() {
                        if let Err(e) = self_cp0.register(key, sc).await {
                            panic!("etcd register failed: {}", e.to_string());
                        }
                    }
                }
            };

            let block1 = async move {
                match self_cp2
                    .client
                    .clone()
                    .watch(
                        format!("{}", BACKEND_SERVICE,),
                        Some(WatchOptions::default().with_prefix()),
                    )
                    .await
                {
                    Ok((_, mut stream)) => {
                        while let Ok(Some(resp)) = stream.message().await {
                            for event in resp.events().iter() {
                                match event.event_type() {
                                    etcd_client::EventType::Put => {
                                        let kv = event.kv().unwrap();
                                        let key = kv.key_str().unwrap();
                                        let value = kv.value_str().unwrap();
                                        let mut inner = self_cp2.inner.lock().await;
                                        inner.insert(
                                            key.to_string(),
                                            serde_json::from_str(value).unwrap(),
                                        );
                                    }
                                    etcd_client::EventType::Delete => todo!(),
                                }
                            }
                        }
                    }
                    Err(e) => {
                        panic!("etcd watch failed: {}", e.to_string());
                    }
                }
            };

            tokio::select! {
                _ = block0 => {},
                _ = block1 => {},
                _ = ctx.done() => {
                    if let Err(_) = self_cp1.unregister().await{} {
                        log::error!("etcd unregister failed");
                    }
                    drop(wg.clone());
                },
            }
        };

        tokio::spawn(block);
    }

    async fn web_service_handle(&mut self, ctx: Context, wg: WaitGroup) {
        let mut ctx = ctx;
        let self_cp0 = self.clone();
        let self_cp1 = self.clone();

        let block = async move {
            // auto register every lease-1s
            let block0 = async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        (LEASE - 1 as i64).try_into().unwrap(),
                    ))
                    .await;

                    log::debug!("auto register");

                    let inner = self_cp0.inner.lock().await;

                    for (key, sc) in inner.iter() {
                        if let Err(e) = self_cp0.register(key, sc).await {
                            panic!("etcd register failed: {}", e.to_string());
                        }
                    }
                }
            };

            tokio::select! {
                _ = block0 => {},
                _ = ctx.done() => {
                    if let Err(_) = self_cp1.unregister().await{} {
                        log::error!("etcd unregister failed");
                    }
                    drop(wg.clone());
                },
            }
        };

        tokio::spawn(block);
    }
}
