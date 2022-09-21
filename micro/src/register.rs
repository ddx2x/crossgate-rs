#[derive(Debug)]
pub enum RegisterError {
    RegisterError(String),
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
            let content = plugin::Content {
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

    pub(crate) async fn get_service(
        &self,
        name: &str,
    ) -> anyhow::Result<(crate::LoadBalancerAlgorithm, crate::Endpoint), RegisterError> {
        if let Ok(contents) = plugin::get(name).await {
            let addrs = contents.iter().map(|c| c.addr.clone()).collect();
            let mut lba = "".to_string();
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
