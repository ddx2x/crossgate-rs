use rand::Rng;

pub static DEFAULT_LOAD_BALANCER_ALGORITHM: LoadBalancerAlgorithm =
    LoadBalancerAlgorithm::RoundRobin;

#[derive(Debug, Clone, Copy)]
pub enum LoadBalancerAlgorithm<'a> {
    RoundRobin,
    Random,
    Strict(&'a str),
}

impl<'a> From<String> for LoadBalancerAlgorithm<'a> {
    fn from(s: String) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "round_robin" => LoadBalancerAlgorithm::RoundRobin,
            "random" => LoadBalancerAlgorithm::Random,
            "strict" => LoadBalancerAlgorithm::Strict(""),
            _ => LoadBalancerAlgorithm::RoundRobin, //default return rr
        }
    }
}

impl<'a> std::fmt::Display for LoadBalancerAlgorithm<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadBalancerAlgorithm::RoundRobin => write!(f, "RoundRobin"),
            LoadBalancerAlgorithm::Random => write!(f, "Random"),
            LoadBalancerAlgorithm::Strict(_) => write!(f, "Strict"),
        }
    }
}

static mut N: usize = 0;

impl<'a> LoadBalancerAlgorithm<'a> {
    pub async fn get(&self, addrs: &[String]) -> String {
        match self {
            LoadBalancerAlgorithm::RoundRobin => self.chioce(addrs).await,
            LoadBalancerAlgorithm::Random => self.chioce(addrs).await,
            LoadBalancerAlgorithm::Strict(s) => (*s).to_string(),
        }
    }

    async fn chioce(&self, addrs: &[String]) -> String {
        match self {
            LoadBalancerAlgorithm::RoundRobin => unsafe {
                N = N + 1;
                return addrs[(N - 1) % addrs.len()].clone();
            },
            LoadBalancerAlgorithm::Random => {
                let mut rng = rand::thread_rng();
                let index = rng.gen_range(0..addrs.len());
                let addr = &addrs[index];
                if addr.is_empty() {
                    return "".to_string();
                }
                return addr.to_string();
            }
            LoadBalancerAlgorithm::Strict(_) => todo!(),
        }
    }
}
