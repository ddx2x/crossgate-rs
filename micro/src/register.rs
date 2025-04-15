use crate::{Endpoint, Executor, LoadBalancerAlgorithm, Service};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegisterError {
    #[error("the register for key `{0}` is not available")]
    RegisterError(String),
    #[error("the service for key `{0}` is not available")]
    ServiceError(String),
}

static REGISTER: Register = Register {};

#[derive(Copy, Clone)]
pub struct Register;

impl Default for Register {
    fn default() -> Self {
        REGISTER.clone()
    }
}

impl Register {
    pub(crate) async fn register_web_service(&self, service: &dyn Service) -> anyhow::Result<()> {
        let lba = service.lab().to_string();

        dotenv::dotenv().ok();

        let mut addr = format!(
            "{}:{}",
            local_ip_address::local_ip()?,
            service.addr().port()
        );

        let strict_address = ::std::env::var("STRICT").unwrap_or("".to_string());

        if !strict_address.is_empty() {
            addr = strict_address
        }

        log::info!(
            "registry web service is {} ip {} lba {}",
            service.name(),
            addr,
            service.lab()
        );

        for name in service.name().split(',').collect::<Vec<&str>>() {
            let content = plugin::ServiceContent {
                service: name.to_string(),
                lba: lba.clone(),
                addr: addr.clone(),
                r#type: plugin::ServiceType::WEB,
            };

            plugin::register_service(name, content)
                .await
                .map_err(|e| RegisterError::RegisterError(e.to_string()))?;
        }
        Ok(())
    }

    pub(crate) async fn register_backend_service<'a>(
        &self,
        service: &mut dyn Executor<'a>,
    ) -> anyhow::Result<()> {
        let content = plugin::ServiceContent {
            service: service.group(),
            r#type: 2,
            ..Default::default()
        };

        plugin::register_service(&service.group(), content)
            .await
            .map_err(|e| RegisterError::RegisterError(e.to_string()))?;

        Ok(())
    }

    pub async fn get_backend_service(&self, name: &str) -> anyhow::Result<(String, Vec<String>)> {
        let (id, mut ids) = plugin::get_backend_service(name)
            .await
            .map_err(|_| RegisterError::ServiceError("service not found ".to_string()))?;
        ids.sort();
        Ok((id, ids.to_owned()))
    }

    pub(crate) async fn get_web_service_by_lba<'a>(
        &'a self,
        name: &'a str,
        lba: LoadBalancerAlgorithm,
    ) -> anyhow::Result<(crate::LoadBalancerAlgorithm, Endpoint)> {
        let contents = plugin::get_web_service(name)
            .await
            .map_err(|_| RegisterError::ServiceError("service not found ".to_string()))?;

        let mut filter_contents = vec![];

        match lba.clone() {
            crate::LoadBalancerAlgorithm::RoundRobin => {
                filter_contents.extend(
                    contents
                        .iter()
                        .filter(|item| item.lba == "RoundRobin")
                        .collect::<Vec<&plugin::ServiceContent>>(),
                );
            }
            crate::LoadBalancerAlgorithm::Random => {
                filter_contents.extend(
                    contents
                        .iter()
                        .filter(|item| item.lba == "Random")
                        .collect::<Vec<&plugin::ServiceContent>>(),
                );
            }
            crate::LoadBalancerAlgorithm::Strict(v) => {
                filter_contents.extend(
                    contents
                        .iter()
                        .filter(|item| item.lba == "Strict" && item.addr == v)
                        .collect::<Vec<&plugin::ServiceContent>>(),
                );
            }
        };

        Ok((
            lba,
            crate::Endpoint {
                addr: filter_contents.iter().map(|c| c.addr.clone()).collect(),
            },
        ))
    }

    pub(crate) async fn get_web_service(
        &self,
        name: &str,
    ) -> anyhow::Result<(LoadBalancerAlgorithm, Endpoint)> {
        if let Ok(contents) = plugin::get_web_service(name).await {
            let addrs = contents
                .iter()
                .map(|c: &plugin::ServiceContent| c.addr.clone())
                .collect();
            let mut lba = "".to_string();

            // 如果有多个服务，那么需要按照负载均衡算法优先级选择一个，Strict优先级最高
            if !contents.is_empty() {
                // 其实这里需要按照负载均衡算法优先级选择一个
                lba = contents[0].lba.clone();
            }

            return Ok((
                crate::LoadBalancerAlgorithm::from(lba),
                crate::Endpoint { addr: addrs },
            ));
        }

        Err(anyhow::anyhow!(RegisterError::ServiceError(
            "service not found ".to_string(),
        )))
    }
}
