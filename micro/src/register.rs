use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegisterError {
    #[error("the register for key `{0}` is not available")]
    RegisterError(String),
    #[error("the service for key `{0}` is not available")]
    ServiceError(String),
}

static REGISTER: Register = Register {};

#[derive(Clone)]
pub struct Register;

impl Default for Register {
    fn default() -> Self {
        REGISTER.clone()
    }
}

impl Register {
    pub(crate) async fn register(
        &self,
        service: &dyn crate::Service,
    ) -> anyhow::Result<(), RegisterError> {
        let lba = service.lab().to_string();

        let addr = format!(
            "{}:{}",
            local_ip_address::local_ip().unwrap(),
            service.addr().port()
        );

        for name in service.name().split(',').collect::<Vec<&str>>() {
            let content = plugin::ServiceContent {
                service: name.to_string(),
                lba: lba.clone(),
                addr: addr.clone(),
            };

            plugin::set(name, content)
                .await
                .map_err(|e| RegisterError::RegisterError(e.to_string()))?;
        }
        Ok(())
    }

    pub(crate) async fn get_service_by_lba<'a>(
        &'a self,
        name: &'a str,
        lba: crate::LoadBalancerAlgorithm,
    ) -> anyhow::Result<(crate::LoadBalancerAlgorithm, crate::Endpoint), RegisterError> {
        let contents = plugin::get(name)
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

    pub(crate) async fn get_service(
        &self,
        name: &str,
    ) -> anyhow::Result<(crate::LoadBalancerAlgorithm, crate::Endpoint), RegisterError> {
        if let Ok(contents) = plugin::get(name).await {
            let addrs = contents.iter().map(|c| c.addr.clone()).collect();
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

        Err(RegisterError::ServiceError(
            "service not found ".to_string(),
        ))
    }
}
