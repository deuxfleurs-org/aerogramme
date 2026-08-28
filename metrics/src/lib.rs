use prometheus::{IntCounter, IntGauge, Opts, Registry};
use std::sync::Arc;

// Here we list all metrics used in workspace
lazy_static::lazy_static! {
    pub static ref REGISTRY: Arc<Registry> = Arc::new(Registry::new());

    pub static ref INSTANCES_CREATED: IntCounter = IntCounter::with_opts(
        Opts::new("imap_instances_created_total", "Number of created IMAP sessions since server run")
    ).unwrap();

    pub static ref INSTANCES_CURRENT: IntGauge = IntGauge::with_opts(
        Opts::new("imap_instances_current", "Number of current IMAP active sessions")
    ).unwrap();
}

pub fn register_all() -> Result<(), prometheus::Error> {
    REGISTRY.register(Box::new(INSTANCES_CREATED.clone()))?;
    REGISTRY.register(Box::new(INSTANCES_CURRENT.clone()))?;
    Ok(())
}
