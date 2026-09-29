use super::{MDNS_SERVICE_NAME, Nearby, node_fingerprint};
use iroh::EndpointId;
use iroh::address_lookup::{AddressLookup, EndpointData, Error, Item};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use n0_future::{StreamExt, boxed::BoxStream};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub(super) struct LocalDiscovery {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    running: Option<Running>,
    data: Option<EndpointData>,
    generation: u64,
}

#[derive(Debug)]
struct Running {
    service: MdnsAddressLookup,
    observer: tokio::task::JoinHandle<()>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.observer.abort();
    }
}

impl LocalDiscovery {
    pub(super) fn enabled(&self) -> bool {
        self.state
            .lock()
            .expect("local discovery")
            .running
            .is_some()
    }

    pub(super) fn set_enabled(
        &self,
        enabled: bool,
        id: EndpointId,
        nearby: Nearby,
    ) -> Result<(), crate::error::EngineError> {
        let mut state = self.state.lock().expect("local discovery");
        if state.running.is_some() == enabled {
            return Ok(());
        }
        state.generation = state.generation.wrapping_add(1);
        if !enabled {
            state.running.take();
            nearby.inner.lock().expect("nearby lock").clear();
            return Ok(());
        }
        let service = MdnsAddressLookup::builder()
            .service_name(MDNS_SERVICE_NAME)
            .build(id)
            .map_err(|error| {
                crate::error::EngineError::Consequence(format!(
                    "LAN discovery could not start: {error}"
                ))
            })?;
        if let Some(data) = &state.data {
            service.publish(data);
        }
        let generation = state.generation;
        let weak = Arc::downgrade(&self.state);
        let observing = service.clone();
        let observer = tokio::spawn(async move {
            let mut events = observing.subscribe().await;
            while let Some(event) = events.next().await {
                let Some(state) = weak.upgrade() else { break };
                let state = state.lock().expect("local discovery");
                if state.generation != generation || state.running.is_none() {
                    break;
                }
                match event {
                    iroh_mdns_address_lookup::DiscoveryEvent::Discovered {
                        endpoint_info, ..
                    } => {
                        let id = endpoint_info.endpoint_id;
                        nearby.observe(
                            id.to_string(),
                            node_fingerprint(&id),
                            endpoint_info
                                .data
                                .user_data()
                                .map(|data| data.to_string())
                                .unwrap_or_default(),
                        );
                    }
                    iroh_mdns_address_lookup::DiscoveryEvent::Expired { endpoint_id } => {
                        nearby.forget(&endpoint_id.to_string());
                    }
                    _ => {}
                }
            }
        });
        state.running = Some(Running { service, observer });
        Ok(())
    }
}

impl AddressLookup for LocalDiscovery {
    fn publish(&self, data: &EndpointData) {
        let mut state = self.state.lock().expect("local discovery");
        state.data = Some(data.clone());
        if let Some(running) = &state.running {
            running.service.publish(data);
        }
    }

    fn resolve(&self, id: EndpointId) -> Option<BoxStream<Result<Item, Error>>> {
        self.state
            .lock()
            .expect("local discovery")
            .running
            .as_ref()?
            .service
            .resolve(id)
    }
}
