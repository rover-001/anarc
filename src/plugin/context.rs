use super::{EventMode, EventPayload, Order, event::EventBus, registry::Contribution};
use std::{any::Any, cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};

pub(crate) type Disposer = Box<dyn FnOnce()>;

#[derive(Clone, Copy)]
pub(crate) struct ServiceKey<S: 'static> {
    pub(super) name: &'static str,
    marker: PhantomData<fn() -> S>,
}

impl<S: 'static> ServiceKey<S> {
    pub(crate) const fn new(name: &'static str) -> Self {
        Self {
            name,
            marker: PhantomData,
        }
    }
}

pub(super) struct ServiceEntry {
    owner: String,
    pub(super) value: Rc<dyn Any>,
}

pub(crate) struct Shared {
    pub(super) services: RefCell<HashMap<String, ServiceEntry>>,
    pub(super) events: RefCell<EventBus>,
    pub(super) contributions: RefCell<Vec<Contribution>>,
}

impl Shared {
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            services: RefCell::new(HashMap::new()),
            events: RefCell::new(EventBus::default()),
            contributions: RefCell::new(Vec::new()),
        })
    }
}

pub(crate) struct PluginCtx<'a> {
    pub(crate) owner: String,
    pub(crate) shared: Rc<Shared>,
    pub(crate) disposers: &'a mut Vec<Disposer>,
}

impl PluginCtx<'_> {
    pub(crate) fn provide<S: Any>(&mut self, key: ServiceKey<S>, service: S) {
        let owner = self.owner.clone();
        self.shared.services.borrow_mut().insert(
            key.name.to_string(),
            ServiceEntry {
                owner: owner.clone(),
                value: Rc::new(service),
            },
        );
        let shared = self.shared.clone();
        let name = key.name.to_string();
        self.disposers.push(Box::new(move || {
            let mut services = shared.services.borrow_mut();
            if services
                .get(&name)
                .is_some_and(|entry| entry.owner == owner)
            {
                services.remove(&name);
            }
        }));
    }

    pub(crate) fn get<S: Any>(&self, key: ServiceKey<S>) -> Option<Rc<S>> {
        self.shared
            .services
            .borrow()
            .get(key.name)
            .and_then(|entry| entry.value.clone().downcast::<S>().ok())
    }

    pub(crate) fn on(
        &mut self,
        name: impl Into<String>,
        mode: EventMode,
        order: Order,
        handler: impl Fn(EventPayload) -> Option<EventPayload> + 'static,
    ) {
        let name = name.into();
        let id = self.shared.events.borrow_mut().on(
            self.owner.clone(),
            name.clone(),
            mode,
            order,
            handler,
        );
        let events = self.shared.clone();
        self.disposers.push(Box::new(move || {
            events.events.borrow_mut().remove(&name, id);
        }));
    }

    pub(crate) fn contribute(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
        order: Order,
    ) {
        let item = Contribution {
            plugin_id: self.owner.clone(),
            key: key.into(),
            value: value.into(),
            order,
        };
        self.shared.contributions.borrow_mut().push(item.clone());
        let shared = self.shared.clone();
        self.disposers.push(Box::new(move || {
            shared
                .contributions
                .borrow_mut()
                .retain(|entry| entry != &item);
        }));
    }

    pub(crate) fn effect(&mut self, dispose: impl FnOnce() + 'static) {
        self.disposers.push(Box::new(dispose));
    }
}
