use crossbeam::sync::WaitGroup;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Server};
use hyper::{Request, Response, StatusCode};
use plugin::get_plugin_type;
use plugin::PluginType::Mongodb;
use tokio_context::context::Context;

use std::convert::Infallible;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;

use crate::{Endpoint, Register};

#[inline]
async fn hash_node<'a>(lba: crate::LoadBalancerAlgorithm<'a>, endpoint: Endpoint) -> String {
    lba.get(endpoint.get_address().as_slice()).await
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

pub enum IntercepterType {
    SelfHandle,
    Redirect,
    NotAuthorized,
    Next,
}

pub type Intercepter = fn(
    r: &mut Request<Body>,
    w: &mut Response<Body>,
)
    -> Pin<Box<dyn Future<Output = IntercepterType> + Send + Sync + 'static>>;

pub fn _default_intercept(_: &Request<Body>, _: &mut Response<Body>) -> IntercepterType {
    IntercepterType::SelfHandle
}

pub type ServeHTTP = fn(req: &Request<Body>) -> anyhow::Result<Response<Body>>;

pub fn default_serve_http(_: &Request<Body>) -> anyhow::Result<Response<Body>> {
    Ok(Response::new(Body::from(TITLE)))
}

fn default_response() -> Response<Body> {
    Response::new(Body::from(TITLE))
}

async fn intercept(
    register: &Register,
    client_ip: IpAddr,
    mut req: Request<Body>,
    intercepters: &'static [Intercepter],
    self_handle: Option<ServeHTTP>,
) -> anyhow::Result<Response<Body>> {
    for intercepter in intercepters {
        let res = &mut Response::new(Body::empty());
        let res = intercepter(&mut req, res).await;
        match res {
            IntercepterType::SelfHandle => {
                // let self_handle = self_handle.unwrap_or(default_serve_http);
                // return self_handle(&req);
            }
            IntercepterType::Redirect => {
                break;
            }
            IntercepterType::NotAuthorized => {
                return Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::empty())
                    .unwrap());
            }
            IntercepterType::Next => {
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

pub async fn run(addr: String, intercepters: &'static [Intercepter], sh: Option<ServeHTTP>) {
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

    let serve = async move {
        let register = &Register {};
        let make_svc = make_service_fn(|conn: &AddrStream| {
            let remote_addr = conn.remote_addr().ip();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    intercept(register, remote_addr, req, intercepters, sh)
                }))
            }
        });

        log::info!("Listening on {}", addr);

        Server::bind(&addr.parse::<SocketAddr>().expect("invalid address"))
            .serve(make_svc)
            .await
            .unwrap();
    };
    tokio::select! {
        _ = serve => {},
        _ = tokio::signal::ctrl_c() => {
            handle.cancel();
            wg.wait();
        },
    }
}
