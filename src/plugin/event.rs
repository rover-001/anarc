use super::registry::{Contribution, Order, sort};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EventMode {
    Emit,
    Parallel,
    Serial,
    Waterfall,
    Bail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EventPayload {
    Empty,
    Text(String),
    Number(i64),
    Flag(bool),
}

type Handler = Box<dyn Fn(EventPayload) -> Option<EventPayload>>;

struct Listener {
    id: u64,
    plugin_id: String,
    order: Order,
    handler: Handler,
}

#[derive(Default)]
pub(crate) struct EventBus {
    listeners: HashMap<String, (EventMode, Vec<Listener>)>,
    next_id: u64,
}

impl EventBus {
    pub(crate) fn on(
        &mut self,
        plugin_id: String,
        name: String,
        mode: EventMode,
        order: Order,
        handler: impl Fn(EventPayload) -> Option<EventPayload> + 'static,
    ) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let (registered_mode, listeners) = self
            .listeners
            .entry(name)
            .or_insert_with(|| (mode, Vec::new()));
        assert_eq!(*registered_mode, mode, "event mode must be consistent");
        listeners.push(Listener {
            id,
            plugin_id,
            order,
            handler: Box::new(handler),
        });
        id
    }

    pub(crate) fn remove(&mut self, name: &str, id: u64) {
        if let Some((_, listeners)) = self.listeners.get_mut(name) {
            listeners.retain(|listener| listener.id != id);
            if listeners.is_empty() {
                self.listeners.remove(name);
            }
        }
    }

    pub(crate) fn emit(
        &self,
        name: &str,
        mode: EventMode,
        payload: EventPayload,
    ) -> Vec<EventPayload> {
        let Some((registered_mode, listeners)) = self.listeners.get(name) else {
            return Vec::new();
        };
        assert_eq!(*registered_mode, mode, "event mode must be consistent");
        let mut keys = listeners
            .iter()
            .map(|listener| Contribution {
                plugin_id: listener.plugin_id.clone(),
                key: String::new(),
                value: String::new(),
                order: listener.order.clone(),
            })
            .collect::<Vec<_>>();
        sort(&mut keys);
        let mut remaining = listeners.iter().collect::<Vec<_>>();
        let ordered = keys
            .iter()
            .map(|key| {
                let index = remaining
                    .iter()
                    .position(|listener| {
                        listener.plugin_id == key.plugin_id && listener.order == key.order
                    })
                    .expect("listener metadata must remain registered");
                &remaining.remove(index).handler
            })
            .collect::<Vec<_>>();

        match mode {
            EventMode::Emit => {
                for handler in ordered {
                    let _ = handler(payload.clone());
                }
                Vec::new()
            }
            EventMode::Parallel | EventMode::Serial => ordered
                .into_iter()
                .filter_map(|handler| handler(payload.clone()))
                .collect(),
            EventMode::Waterfall => {
                let value = ordered.into_iter().fold(payload, |value, handler| {
                    handler(value.clone()).unwrap_or(value)
                });
                vec![value]
            }
            EventMode::Bail => ordered
                .into_iter()
                .find_map(|handler| handler(payload.clone()))
                .into_iter()
                .collect(),
        }
    }
}
