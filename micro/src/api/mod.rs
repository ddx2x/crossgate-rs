use crossbeam::sync::WaitGroup;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Server};
use hyper::{Request, Response, StatusCode};
use plugin::get_plugin_type;
use plugin::PluginType::Mongodb;
use tokio_context::context::Context;

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

use crate::{Endpoint, Register};

#[inline]
async fn hash_node<'a>(lba: crate::LoadBalancerAlgorithm<'a>, endpoint: Endpoint) -> String {
    lba.get(endpoint.get_address().as_slice()).await
}

pub enum InterceptType {
    SelfHandle,
    Redirect,
    NotAuthorized,
    Next,
}

static TITLE: &str = r#"
<html>
<head>
<style type=text/css>
</style>
</head>
<body>
<p> this page is crossgate api gateway.</p>
<br><br> 
</body>
</html>
"#;

pub type Intercept = fn(req: &Request<Body>, w: &mut Response<Body>) -> InterceptType;

pub fn _default_intercept(_: &Request<Body>, _: &mut Response<Body>) -> InterceptType {
    InterceptType::SelfHandle
}

pub type ServeHTTP = fn(req: &Request<Body>) -> Result<Response<Body>, Infallible>;

pub fn default_serve_http(_: &Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::from(TITLE)))
}

fn default_response() -> Response<Body> {
    Response::new(Body::from(TITLE))
}

async fn handle(
    register: &Register,
    client_ip: IpAddr,
    req: Request<Body>,
    intercepts: &[Intercept],
    self_handle: Option<ServeHTTP>,
) -> Result<Response<Body>, Infallible> {
    for intercept in intercepts {
        let res = intercept(&req, &mut Response::new(Body::empty()));
        match res {
            InterceptType::SelfHandle => {
                let self_handle = self_handle.unwrap_or(default_serve_http);
                return self_handle(&req);
            }
            InterceptType::Redirect => {
                break;
            }
            InterceptType::NotAuthorized => {
                return Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::empty())
                    .unwrap());
            }
            InterceptType::Next => {
                continue;
            }
        }
    }

    if req.uri().path() == "/" {
        return Ok(default_response());
    }

    let service_name = req.uri().path().split("/").nth(1).unwrap_or("");

    if service_name == "" {
        return Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body("service unavailable or not found".into())
            .unwrap());
    }

    let (lba, endpoint) = match register.get_service(service_name).await {
        Ok(endpoint) => endpoint,
        Err(e) => {
            log::error!("service not found, error: {:?}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap());
        }
    };

    if 0 == endpoint.get_address().len() {
        log::warn!("not found service: {}", service_name);
        return Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body("service unavailable".into())
            .unwrap());
    }

    let forward_addr = format!("http://{}", hash_node(lba, endpoint).await);

    match net::get_proxy_client()
        .call(client_ip, &forward_addr, req)
        .await
    {
        Ok(res) => {
            return Ok(res);
        }
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap());
        }
    }
}

async fn _run(addr: String, intercepts: &'static [Intercept], sh: Option<ServeHTTP>) {
    let register = &Register {};
    let make_svc = make_service_fn(|conn: &AddrStream| {
        let remote_addr = conn.remote_addr().ip();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                handle(register, remote_addr, req, intercepts, sh)
            }))
        }
    });

    log::info!("Listening on {}", addr);

    Server::bind(&addr.parse::<SocketAddr>().expect("invalid address"))
        .serve(make_svc)
        .await
        .unwrap();
}

pub async fn run(addr: String, intercepts: &'static [Intercept], sh: Option<ServeHTTP>) {
    dotenv::dotenv().ok();

    let (ctx, handle) = Context::new();
    let wg = WaitGroup::new();

    let register_type_name =
        ::std::env::var("REGISTER_TYPE").unwrap_or_else(|_| Mongodb.as_str().into());

    plugin::init_plugin(
        ctx,
        wg.clone(),
        plugin::ServiceType::ApiGateway,
        get_plugin_type(&register_type_name),
    )
    .await;

    tokio::select! {
        _ = _run(addr,intercepts,sh) => {},
        _ = tokio::signal::ctrl_c() => {
            handle.cancel();
            wg.wait();
        },
    }
}
