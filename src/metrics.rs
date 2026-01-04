use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

pub fn install_recorder() -> anyhow::Result<PrometheusHandle> {
    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder()?;
    Ok(handle)
}
