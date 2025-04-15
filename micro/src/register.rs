use crate::{Endpoint, Executor, LoadBalancerAlgorithm, Service};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegisterError {
    #[error("Failed to register service: {0}")]
    RegistrationFailed(String),
    #[error("Service not found: {0}")]
    ServiceNotFound(String),
    #[error("Invalid service configuration: {0}")]
    InvalidConfiguration(String),
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
        let load_balancer_algorithm = service.load_balancer_algorithm().to_string();
        let mut service_address = format!(
            "{}:{}",
            local_ip_address::local_ip()?,
            service.addr().port()
        );

        // Check for strict address override
        if let Ok(strict_address) = ::std::env::var("STRICT") {
            if !strict_address.is_empty() {
                service_address = strict_address;
            }
        }

        log::info!(
            "Registering web service: name={}, address={}, load_balancer={}",
            service.name(),
            service_address,
            service.load_balancer_algorithm()
        );

        for service_name in service.name().split(',') {
            let content = plugin::ServiceContent {
                service: service_name.to_string(),
                lba: load_balancer_algorithm.clone(),
                addr: service_address.clone(),
                r#type: 1,
            };

            plugin::register_service(service_name, content)
                .await
                .map_err(|e| RegisterError::RegistrationFailed(e.to_string()))?;
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
            .map_err(|e| RegisterError::RegistrationFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn get_backend_service(&self, name: &str) -> anyhow::Result<(String, Vec<String>)> {
        let (service_id, mut service_ids) = plugin::get_backend_service(name)
            .await
            .map_err(|_| RegisterError::ServiceNotFound(name.to_string()))?;

        service_ids.sort();
        Ok((service_id, service_ids))
    }

    pub(crate) async fn get_web_service_by_algorithm<'a>(
        &'a self,
        name: &'a str,
        algorithm: &LoadBalancerAlgorithm,
    ) -> anyhow::Result<(LoadBalancerAlgorithm, Endpoint)> {
        let services = plugin::get_web_service(name)
            .await
            .map_err(|_| RegisterError::ServiceNotFound(name.to_string()))?;

        let filtered_services = match algorithm {
            LoadBalancerAlgorithm::RoundRobin => services
                .iter()
                .filter(|item| item.lba == "RoundRobin")
                .collect::<Vec<&plugin::ServiceContent>>(),
            LoadBalancerAlgorithm::Random => services
                .iter()
                .filter(|item| item.lba == "Random")
                .collect::<Vec<&plugin::ServiceContent>>(),
            LoadBalancerAlgorithm::Strict(address) => services
                .iter()
                .filter(|item| item.lba == "Strict" && item.addr == address.as_str())
                .collect::<Vec<&plugin::ServiceContent>>(),
        };

        Ok((
            algorithm.clone(),
            Endpoint::new(filtered_services.iter().map(|c| c.addr.clone()).collect()),
        ))
    }

    pub(crate) async fn get_web_service(
        &self,
        name: &str,
    ) -> anyhow::Result<(LoadBalancerAlgorithm, Endpoint)> {
        let services = plugin::get_web_service(name)
            .await
            .map_err(|_| RegisterError::ServiceNotFound(name.to_string()))?;

        if services.is_empty() {
            return Err(RegisterError::ServiceNotFound(name.to_string()).into());
        }

        let addresses = services.iter().map(|c| c.addr.clone()).collect();
        let algorithm = LoadBalancerAlgorithm::from(services[0].lba.clone());

        Ok((algorithm, Endpoint::new(addresses)))
    }
}
