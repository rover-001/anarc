use super::PluginCtx;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Manifest {
    pub(crate) id: String,
    pub(crate) required: Vec<String>,
    pub(crate) optional: Vec<String>,
    pub(crate) provides: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginError(pub(crate) String);

pub(crate) trait Plugin: 'static {
    fn manifest(&self) -> &Manifest;
    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError>;
}
