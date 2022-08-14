use crate::Managed;

#[repr(C)]
pub struct PluginInstance {
    pub instance: Managed,
    pub should_drop: bool,
    pub vtb: PluginVTable,
}

impl PluginInstance {
    pub fn from(instance: Managed, should_drop: bool,vtb: PluginVTable) -> Self {
        Self { instance, should_drop,vtb }
    }

    pub fn enable(&self) {
        (self.vtb.enable)(self.instance.pointer)
    }

    pub fn disable(&self) {
        (self.vtb.disable)(self.instance.pointer)
    }
}

#[repr(C)]
pub struct PluginVTable {
    pub new: extern "C" fn() -> Managed,
    pub enable: extern "C" fn(*mut ()),
    pub disable: extern "C" fn(*mut ()),
}