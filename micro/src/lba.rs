use rand::{rngs::ThreadRng, Rng};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::collections::HashSet;

pub static DEFAULT_LOAD_BALANCER_ALGORITHM: LoadBalancerAlgorithm = LoadBalancerAlgorithm::RoundRobin;

#[derive(Debug, Clone)]
pub enum LoadBalancerAlgorithm {
    RoundRobin,
    Random,
    Strict(Arc<String>),
}

impl LoadBalancerAlgorithm {
    pub fn from_environment() -> Self {
        if let Ok(strict_address) = std::env::var("STRICT") {
            if !strict_address.is_empty() {
                return LoadBalancerAlgorithm::Strict(Arc::new(strict_address));
            }
        }
        LoadBalancerAlgorithm::RoundRobin
    }

    pub fn select_address(&self, addresses: &[String]) -> Option<String> {
        if addresses.is_empty() {
            return None;
        }

        match self {
            LoadBalancerAlgorithm::RoundRobin => {
                static COUNTER: AtomicUsize = AtomicUsize::new(0);
                let index = COUNTER.fetch_add(1, Ordering::Relaxed) % addresses.len();
                Some(addresses[index].clone())
            }
            LoadBalancerAlgorithm::Random => {
                let mut rng = rand::thread_rng();
                let index = rng.gen_range(0..addresses.len());
                Some(addresses[index].clone())
            }
            LoadBalancerAlgorithm::Strict(address) => {
                // Use HashSet for O(1) lookup
                let address_set: HashSet<&str> = addresses.iter().map(|s| s.as_str()).collect();
                if address_set.contains(address.as_str()) {
                    Some(address.to_string())
                } else {
                    None
                }
            }
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            LoadBalancerAlgorithm::RoundRobin => "RoundRobin".to_string(),
            LoadBalancerAlgorithm::Random => "Random".to_string(),
            LoadBalancerAlgorithm::Strict(_) => "Strict".to_string(),
        }
    }
}

impl From<String> for LoadBalancerAlgorithm {
    fn from(s: String) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "roundrobin" => LoadBalancerAlgorithm::RoundRobin,
            "random" => LoadBalancerAlgorithm::Random,
            "strict" => LoadBalancerAlgorithm::Strict(Arc::new("".into())),
            _ => LoadBalancerAlgorithm::RoundRobin,
        }
    }
}

impl std::fmt::Display for LoadBalancerAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
