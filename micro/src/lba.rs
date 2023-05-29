use rand::{rngs::ThreadRng, Rng};

pub static DEFAULT_LOAD_BALANCER_ALGORITHM: LoadBalancerAlgorithm =
    LoadBalancerAlgorithm::RoundRobin;

#[derive(Debug, Clone)]
pub enum LoadBalancerAlgorithm {
    RoundRobin,
    Random,
    Strict(String),
}

impl From<String> for LoadBalancerAlgorithm {
    fn from(s: String) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "RoundRobin" => LoadBalancerAlgorithm::RoundRobin,
            "Random" => LoadBalancerAlgorithm::Random,
            "Strict" => LoadBalancerAlgorithm::Strict("".into()),
            _ => LoadBalancerAlgorithm::RoundRobin, //default return rr
        }
    }
}

impl std::fmt::Display for LoadBalancerAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadBalancerAlgorithm::RoundRobin => write!(f, "RoundRobin"),
            LoadBalancerAlgorithm::Random => write!(f, "Random"),
            LoadBalancerAlgorithm::Strict(_) => write!(f, "Strict"),
        }
    }
}

static mut N: usize = 0;

impl LoadBalancerAlgorithm {
    pub async fn hash(&self, addrs: &[String]) -> String {
        match self {
            LoadBalancerAlgorithm::RoundRobin => unsafe {
                N = N + 1;
                return addrs[(N - 1) % addrs.len()].clone();
            },
            LoadBalancerAlgorithm::Random => {
                return addrs[rand::thread_rng().gen_range(0..addrs.len())].to_string();
            }
            LoadBalancerAlgorithm::Strict(s) => {
                if !addrs.contains(s) {
                    return "".into();
                }
                return s.clone();
            }
        }
    }
}
