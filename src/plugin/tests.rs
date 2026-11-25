use super::*;
use std::{cell::RefCell, rc::Rc};

const NUMBER: ServiceKey<i32> = ServiceKey::new("number");
const DERIVED: ServiceKey<i32> = ServiceKey::new("derived");
type TestAction = dyn FnMut(&mut PluginCtx<'_>) -> Result<(), PluginError>;

struct TestPlugin {
    manifest: Manifest,
    action: Box<TestAction>,
}

impl Plugin for TestPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        (self.action)(ctx)
    }
}

fn plugin(
    id: &str,
    required: &[&str],
    provides: &[&str],
    action: impl FnMut(&mut PluginCtx<'_>) -> Result<(), PluginError> + 'static,
) -> Box<dyn Plugin> {
    Box::new(TestPlugin {
        manifest: Manifest {
            id: id.into(),
            required: required.iter().map(|value| (*value).into()).collect(),
            optional: Vec::new(),
            provides: provides.iter().map(|value| (*value).into()).collect(),
        },
        action: Box::new(action),
    })
}

#[test]
fn pending_plugins_activate_when_required_service_arrives_and_cascade_on_removal() {
    let mut kernel = Kernel::new();
    let consumer_applies = Rc::new(RefCell::new(0));
    let applies = consumer_applies.clone();
    kernel
        .mount(plugin("consumer", &["number"], &[], move |_| {
            *applies.borrow_mut() += 1;
            Ok(())
        }))
        .unwrap();
    assert_eq!(kernel.state("consumer"), Some(&FiberState::Pending));

    kernel
        .mount(plugin("leaf", &["derived"], &[], |_| Ok(())))
        .unwrap();
    kernel
        .mount(plugin("middle", &["number"], &["derived"], |ctx| {
            ctx.provide(DERIVED, 84);
            Ok(())
        }))
        .unwrap();
    assert_eq!(kernel.state("middle"), Some(&FiberState::Pending));
    assert_eq!(kernel.state("leaf"), Some(&FiberState::Pending));

    kernel
        .mount(plugin("provider", &[], &["number"], |ctx| {
            ctx.provide(NUMBER, 42);
            Ok(())
        }))
        .unwrap();
    assert_eq!(kernel.state("consumer"), Some(&FiberState::Active));
    assert_eq!(*consumer_applies.borrow(), 1);

    kernel.unmount("provider");
    assert_eq!(kernel.state("consumer"), Some(&FiberState::Pending));
    assert_eq!(kernel.state("middle"), Some(&FiberState::Pending));
    assert_eq!(kernel.state("leaf"), Some(&FiberState::Pending));
    assert_eq!(kernel.state("provider"), Some(&FiberState::Disposed));

    kernel
        .mount(plugin("provider", &[], &["number"], |ctx| {
            ctx.provide(NUMBER, 7);
            Ok(())
        }))
        .unwrap();
    assert_eq!(kernel.state("consumer"), Some(&FiberState::Active));
    assert_eq!(kernel.state("middle"), Some(&FiberState::Active));
    assert_eq!(kernel.state("leaf"), Some(&FiberState::Active));
}

#[test]
fn teardown_runs_disposers_in_reverse_registration_order() {
    let mut kernel = Kernel::new();
    let order = Rc::new(RefCell::new(Vec::new()));
    let observed = order.clone();
    kernel
        .mount(plugin("effects", &[], &[], move |ctx| {
            let first = observed.clone();
            ctx.effect(move || first.borrow_mut().push(1));
            let second = observed.clone();
            ctx.effect(move || second.borrow_mut().push(2));
            Ok(())
        }))
        .unwrap();
    kernel.unmount("effects");
    assert_eq!(*order.borrow(), vec![2, 1]);
}

#[test]
fn event_modes_apply_their_distinct_dispatch_rules() {
    let mut bus = EventBus::default();
    for (id, value) in [("b", 2), ("a", 1)] {
        bus.on(
            id.into(),
            "emit".into(),
            EventMode::Emit,
            Order::default(),
            move |_| Some(EventPayload::Number(value)),
        );
        bus.on(
            id.into(),
            "bail".into(),
            EventMode::Bail,
            Order::default(),
            move |_| (value == 2).then_some(EventPayload::Number(value)),
        );
    }
    bus.on(
        "a".into(),
        "waterfall".into(),
        EventMode::Waterfall,
        Order::default(),
        |value| match value {
            EventPayload::Number(value) => Some(EventPayload::Number(value + 1)),
            _ => None,
        },
    );
    bus.on(
        "b".into(),
        "waterfall".into(),
        EventMode::Waterfall,
        Order::default(),
        |value| match value {
            EventPayload::Number(value) => Some(EventPayload::Number(value * 2)),
            _ => None,
        },
    );
    for mode in [EventMode::Parallel, EventMode::Serial] {
        bus.on(
            "a".into(),
            format!("{mode:?}"),
            mode,
            Order::default(),
            |_| Some(EventPayload::Flag(true)),
        );
        bus.on(
            "b".into(),
            format!("{mode:?}"),
            mode,
            Order::default(),
            |_| Some(EventPayload::Flag(false)),
        );
    }

    assert_eq!(
        bus.emit("emit", EventMode::Emit, EventPayload::Empty),
        Vec::<EventPayload>::new()
    );
    assert_eq!(
        bus.emit("Parallel", EventMode::Parallel, EventPayload::Empty)
            .len(),
        2
    );
    assert_eq!(
        bus.emit("Serial", EventMode::Serial, EventPayload::Empty)
            .len(),
        2
    );
    assert_eq!(
        bus.emit("waterfall", EventMode::Waterfall, EventPayload::Number(1)),
        vec![EventPayload::Number(4)]
    );
    assert_eq!(
        bus.emit("bail", EventMode::Bail, EventPayload::Empty),
        vec![EventPayload::Number(2)]
    );
}

#[test]
fn ordering_ties_break_by_plugin_id_and_panics_fail_only_the_fiber() {
    let mut items = vec![
        Contribution {
            plugin_id: "zeta".into(),
            key: "x".into(),
            value: "".into(),
            order: Order::Priority(4),
        },
        Contribution {
            plugin_id: "alpha".into(),
            key: "x".into(),
            value: "".into(),
            order: Order::Priority(4),
        },
    ];
    registry::sort(&mut items);
    assert_eq!(items[0].plugin_id, "alpha");

    let mut constrained = vec![
        Contribution {
            plugin_id: "alpha".into(),
            key: "x".into(),
            value: "".into(),
            order: Order::After("zeta".into()),
        },
        Contribution {
            plugin_id: "zeta".into(),
            key: "x".into(),
            value: "".into(),
            order: Order::Priority(4),
        },
    ];
    registry::sort(&mut constrained);
    assert_eq!(constrained[0].plugin_id, "zeta");
    assert_eq!(constrained[1].plugin_id, "alpha");

    let mut kernel = Kernel::new();
    kernel
        .mount(plugin("panic", &[], &[], |_| panic!("controlled panic")))
        .unwrap();
    assert_eq!(
        kernel.state("panic"),
        Some(&FiberState::Failed("controlled panic".into()))
    );
}

#[test]
fn core_palette_and_shortcut_registrations_match_the_baseline_snapshot() {
    let mut kernel = Kernel::new();
    kernel.mount(Box::new(CorePlugin::new())).unwrap();
    let contributions = kernel.contributions();
    let palette = contributions
        .iter()
        .filter(|item| item.key == "palette.action")
        .map(|item| item.value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        palette,
        [
            "new-tab",
            "close-tab",
            "reopen-closed-tab",
            "reload",
            "find-in-page",
            "zoom-in",
            "zoom-out",
            "zoom-reset",
            "new-space",
            "enable-plugin",
            "disable-plugin",
            "reload-plugins",
            "open-plugin-folder",
            "telemetry-preview",
            "telemetry-reset-install-id",
        ]
    );

    let shortcuts = contributions
        .iter()
        .filter(|item| item.key == "keymap.binding")
        .map(|item| item.value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        shortcuts,
        [
            "ctrl+l=focus-address",
            "ctrl+shift+b=toggle-shield",
            "ctrl+k=command-palette",
            "ctrl+f=find-in-page",
            "ctrl+shift+f=find-in-page",
            "ctrl+t=new-tab",
            "ctrl+shift+t=reopen-closed-tab",
            "ctrl+w=close-tab",
            "ctrl+tab=next-tab",
            "ctrl+shift+tab=previous-tab",
            "ctrl+r=reload",
            "ctrl+shift+r=hard-reload",
            "ctrl+==zoom-in",
            "ctrl+shift+==zoom-in",
            "ctrl++=zoom-in",
            "ctrl+shift++=zoom-in",
            "ctrl+-=zoom-out",
            "ctrl+shift+-=zoom-out",
            "ctrl+0=zoom-reset",
            "ctrl+shift+0=zoom-reset",
            "ctrl+1..9=activate-tab-number",
            "ctrl+shift+s=split-view",
            "ctrl+shift+w=close-window",
        ]
    );
}
