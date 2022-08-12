use crate::Managed;

#[repr(C)]
pub struct PluginInstance {
    instance: Managed,
    vtb: PluginVTable,
}

impl PluginInstance {
    pub fn from(m: Managed, vtb: PluginVTable) -> Self {
        Self {
            instance: m,
            vtb
        }
    }
    
    pub fn enable(&self) {
        (self.vtb.enable)(self.instance.pointer)
    }
}

#[repr(C)]
pub struct PluginVTable {
    enable: extern fn(*mut ()),
    disable: extern fn(*mut ())
}

impl PluginVTable {
    pub fn from(enable: extern fn(*mut ()),disable: extern fn(*mut ())) -> Self {
        Self {
            enable,
            disable
        }
    }
}