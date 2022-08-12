use crate::Managed;

#[repr(C)]
pub struct PluginInstance {
    instance: Managed,
    vtb: PluginVTable,
}

impl PluginInstance {
    pub fn from(instance: Managed, vtb: PluginVTable) -> Self {
        Self { instance, vtb }
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
    enable: extern "C" fn(*mut ()),
    disable: extern "C" fn(*mut ()),
}

impl PluginVTable {
    pub fn from(enable: extern "C" fn(*mut ()), disable: extern "C" fn(*mut ())) -> Self {
        Self { enable, disable }
    }
}
