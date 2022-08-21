use crate::loader::get_plugin_manager_vtb;
use atri_ffi::ManagedCloneable;
use std::fmt::{Display, Formatter};

#[derive(Clone)]
pub struct Bot(pub(crate) ManagedCloneable);

impl Bot {
    pub fn id(&self) -> i64 {
        (get_plugin_manager_vtb().bot_get_id)(self.0.pointer)
    }

    pub fn nickname(&self) -> &str {
        let rs = (get_plugin_manager_vtb().bot_get_nickname)(self.0.pointer);

        rs.as_str()
    }
}

impl Display for Bot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bot({})", self.id())
    }
}
