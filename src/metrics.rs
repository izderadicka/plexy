use opentelemetry::metrics::{Meter, MeterProvider};

pub fn init_meter() -> Meter {
    let meter_provider = opentelemetry::global::meter_provider();
    let meter = meter_provider.meter("plexy");
    meter
}

use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server,
};

use opentelemetry_prometheus::PrometheusExporter;
use prometheus::{Encoder, Registry, TextEncoder};
use std::{convert::Infallible, net::SocketAddr};

use opentelemetry::sdk::export::metrics::aggregation;
use opentelemetry::sdk::metrics::{controllers, processors, selectors};

async fn serve_req(req: Request<Body>, registry: Registry) -> Result<Response<Body>, hyper::Error> {
    let response = match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            let mut buffer = vec![];
            let encoder = TextEncoder::new();
            let metric_families = registry.gather();
            encoder.encode(&metric_families, &mut buffer).unwrap();

            Response::builder()
                .status(200)
                .header(CONTENT_TYPE, encoder.format_type())
                .body(Body::from(buffer))
                .unwrap()
        }

        _ => Response::builder()
            .status(404)
            .body(Body::from("Not Found"))
            .unwrap(),
    };

    Ok(response)
}

pub fn init_prometheus() -> (PrometheusExporter, Registry) {
    let registry = Registry::new();
    let controller = controllers::basic(processors::factory(
        selectors::simple::histogram([1.0, 2.0, 5.0, 10.0, 20.0, 50.0]),
        aggregation::cumulative_temporality_selector(),
    ))
    .build();
    let exporter = opentelemetry_prometheus::exporter(controller)
        .with_registry(registry.clone())
        .init();
    (exporter, registry)
}

pub async fn run(
    addr: SocketAddr,
    registry: Registry,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // For every connection, we must make a `Service` to handle all
    // incoming HTTP requests on said connection.
    let make_svc = make_service_fn(move |_conn| {
        let state = registry.clone();
        // This is the `Service` that will handle the connection.
        // `service_fn` is a helper to convert a function that
        // returns a Response into a `Service`.
        async move { Ok::<_, Infallible>(service_fn(move |req| serve_req(req, state.clone()))) }
    });

    let server = Server::bind(&addr).serve(make_svc);
    server.await?;

    Ok(())
}
