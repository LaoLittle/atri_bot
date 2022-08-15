use atri_ffi::Managed;
use crate::loader::get_plugin_manager_vtb;

pub struct Bot(pub(crate) Managed);

impl Bot {
    pub fn id(&self) -> i64 {
        (get_plugin_manager_vtb().bot_get_id)(self.0.pointer)
    }
}