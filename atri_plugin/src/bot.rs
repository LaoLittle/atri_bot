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

    pub fn list() -> Vec<Bot> {
        let raw = (get_plugin_manager_vtb().bot_get_list)();

        raw.into_vec().into_iter().map(Bot).collect()
    }

    pub fn find(id: i64) -> Option<Self> {
        let ma = (get_plugin_manager_vtb().find_bot)(id);

        if ma.pointer.is_null() { None }
        else { Some(Self(ma)) }
    }

    pub fn find_group(&self, id: i64) -> Option<Bot> {
        let ma = (get_plugin_manager_vtb().bot_find_group)(self.0.pointer, id);

        if ma.pointer.is_null() { None }
        else { Some(Self(ma)) }
    }

    pub fn find_friend(&self, id: i64) -> Option<Bot> {
        let ma = (get_plugin_manager_vtb().bot_find_friend)(self.0.pointer, id);

        if ma.pointer.is_null() { None }
        else { Some(Self(ma)) }
    }
}

impl Display for Bot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bot({})", self.id())
    }
}
