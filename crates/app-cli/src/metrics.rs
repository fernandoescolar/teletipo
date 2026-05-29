use std::net::SocketAddr;

pub(crate) fn install_metrics(enabled: bool) -> Option<metrics_exporter_prometheus::PrometheusHandle> {
    if !enabled {
        return None;
    }

    let addr: SocketAddr = "127.0.0.1:9898".parse().expect("valid metrics socket address");
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new().with_http_listener(addr);
    match builder.install_recorder() {
        Ok(handle) => {
            tracing::info!(address = %addr, "metrics exporter listening");
            Some(handle)
        }
        Err(err) => {
            tracing::warn!(address = %addr, error = %err, "failed to install metrics exporter");
            None
        }
    }
}