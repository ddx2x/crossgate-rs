use crossbeam::sync::WaitGroup;
use futures::future::BoxFuture;
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
    Forbidden, // 403
    Interrupt, // 新增加中断，当由中间件函数处理结果
    Next,
}

pub type Intercepter = for<'a> fn(
    r: &'a mut Request<Body>,
    w: &'a mut Response<Body>,
) -> BoxFuture<'a, IntercepterType>;

pub fn _default_intercept(_: &Request<Body>, _: &mut Response<Body>) -> IntercepterType {
    IntercepterType::SelfHandle
}

pub type ServeHTTP = fn(req: &Request<Body>) -> anyhow::Result<Response<Body>>;

pub fn default_serve_http(_: &Request<Body>) -> anyhow::Result<Response<Body>> {
    Ok(Response::new(Body::from(TITLE)))
}

fn extracting_service(path: &str) -> String {
    let parts: Vec<&str> = path.split("/").collect::<Vec<&str>>().drain(1..).collect();
    if parts.len() < 2 {
        return String::from("");
    }
    format!("/{}/{}", parts[0], parts[1])
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
        let mut res = Response::new(Body::empty());

        match intercepter(&mut req, &mut res).await {
            IntercepterType::SelfHandle => return self_handle.unwrap_or(default_serve_http)(&req),
            IntercepterType::Redirect => break,
            IntercepterType::NotAuthorized => {
                return Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::empty())
                    .unwrap());
            }
            IntercepterType::Forbidden => {
                return Ok(Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Body::empty())
                    .unwrap());
            }
            IntercepterType::Next => continue,
            IntercepterType::Interrupt => return Ok(res),
        }
    }

    if req.uri().path() == "/" {
        return Ok(default_response());
    }

    //  /t/ums/user/login => /t/ums
    let service_name = extracting_service(req.uri().path());
    if service_name == "" {
        return Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body("service unavailable or not found".into())
            .unwrap());
    }

    // 如果请求头中有strict，那么直接转发到strict中
    if let Some(strict) = req.headers().get("strict") {
        let strict_address = strict.to_str().unwrap_or("").to_string();

        if strict_address.is_empty() {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("strict address is empty".into())
                .unwrap());
        }

        let (lba, endpoint) = match register
            .get_service_by_lba(
                &service_name,
                crate::LoadBalancerAlgorithm::Strict(strict_address),
            )
            .await
        {
            Ok(endpoint) => endpoint,
            Err(_) => {
                return Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap());
            }
        };

        if endpoint.get_address().is_empty() {
            return Ok(Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(format!("{} not found", service_name).into())
                .unwrap());
        }

        let forward_addr = format!(
            "http://{}",
            lba.hash(endpoint.get_address().as_slice()).await
        );

        match net::get_proxy_client()
            .call(client_ip, &forward_addr, req)
            .await
        {
            Ok(res) => return Ok(res),
            Err(e) => {
                return Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(format!("gateway error: {:#?}", e).into())
                    .unwrap());
            }
        }
    }

    let (lba, endpoint) = match register.get_service(&service_name).await {
        Ok(endpoint) => endpoint,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap());
        }
    };

    if 0 == endpoint.get_address().len() {
        return Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(format!("{} not found", service_name).into())
            .unwrap());
    }

    let forward_addr = format!(
        "http://{}",
        lba.hash(endpoint.get_address().as_slice()).await
    );

    match net::get_proxy_client()
        .call(client_ip, &forward_addr, req)
        .await
    {
        Ok(res) => return Ok(res),
        Err(e) => {
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(format!("gateway error: {:#?}", e).into())
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
