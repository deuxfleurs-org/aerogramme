use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use futures::FutureExt;
use prometheus::{IntCounter, IntGauge, Opts, Registry};
use prometheus_hyper::Server as PrometheusServer;
use tokio::sync::watch;

use aero_user::config::*;

// Here we list all metrics used in workspace
lazy_static::lazy_static! {
    pub static ref INSTANCES_CREATED: IntCounter = IntCounter::with_opts(
        Opts::new("imap_nb_created_sessions", "Number of created IMAP sessions since server run")
    ).unwrap();

    pub static ref INSTANCES_CURRENT: IntGauge = IntGauge::with_opts(
        Opts::new("imap_nb_current_sessions", "Number of current IMAP active sessions")
    ).unwrap();
}

pub struct MetricServer {
  bind_addr: SocketAddr,
  registry: Arc<Registry>,
  
}

impl MetricServer {
  pub fn new(config: PrometheusEndpointConfig) -> Result<Self> {
    let registry = Registry::new();

    // Register all the metrics
    registry.register(Box::new(INSTANCES_CREATED.clone()))?;
    registry.register(Box::new(INSTANCES_CURRENT.clone()))?;

    Ok(Self {
      bind_addr: config.bind_addr,
      registry: Arc::new(registry),
    })
  }

  pub async fn run(self: Self, mut must_exit: watch::Receiver<bool>) -> Result<()> {
    tracing::info!("Metric server available at {:#}", self.bind_addr);
    PrometheusServer::run(
        self.registry,
        self.bind_addr,
        must_exit.changed().map(|_| ()),
    ).await?;
    Ok(())
  }
}

