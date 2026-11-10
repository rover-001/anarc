use super::context::Disposer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FiberState {
    Pending,
    Loading,
    Active,
    Disposed,
    Failed(String),
}

pub(crate) struct Fiber {
    pub(crate) state: FiberState,
    pub(crate) disposers: Vec<Disposer>,
}

impl Fiber {
    pub(crate) fn pending() -> Self {
        Self {
            state: FiberState::Pending,
            disposers: Vec::new(),
        }
    }

    pub(crate) fn dispose_effects(&mut self) {
        while let Some(dispose) = self.disposers.pop() {
            dispose();
        }
    }
}
