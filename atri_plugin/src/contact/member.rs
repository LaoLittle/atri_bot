use crate::bot::Bot;
use atri_ffi::contact::FFIMember;
use atri_ffi::{ManagedCloneable, RustStr};
use std::fmt::{Display, Formatter};
use std::mem::ManuallyDrop;

use crate::contact::group::Group;
use crate::error::AtriError;
use crate::loader::get_plugin_manager_vtb;
use crate::runtime::manager::PluginManager;

#[derive(Clone)]
pub enum Member {
    Named(NamedMember),
    Anonymous(AnonymousMember),
}

impl Member {
    pub(crate) fn from_ffi(ffi: FFIMember) -> Self {
        unsafe {
            if ffi.is_named {
                let named = NamedMember(ManuallyDrop::into_inner(ffi.inner.named));
                Self::Named(named)
            } else {
                let ano = AnonymousMember(ManuallyDrop::into_inner(ffi.inner.anonymous));
                Self::Anonymous(ano)
            }
        }
    }
}

#[derive(Clone)]
pub struct NamedMember(pub(crate) ManagedCloneable);

impl NamedMember {
    pub fn id(&self) -> i64 {
        (get_plugin_manager_vtb().named_member_get_id)(self.0.pointer)
    }

    pub fn nickname(&self) -> &str {
        let rs = (get_plugin_manager_vtb().named_member_get_nickname)(self.0.pointer);

        rs.as_str()
    }

    pub fn card_name(&self) -> &str {
        let rs = (get_plugin_manager_vtb().named_member_get_card_name)(self.0.pointer);

        rs.as_str()
    }

    pub fn group(&self) -> Group {
        let ma = (get_plugin_manager_vtb().named_member_get_group)(self.0.pointer);
        Group(ma)
    }

    pub fn bot(&self) -> Bot {
        self.group().bot()
    }

    pub async fn change_card_name(&self, card_name: &str) -> Result<(), AtriError> {
        let rs = RustStr::from(card_name);

        let fu = (get_plugin_manager_vtb().named_member_change_card_name)(self.0.pointer, rs);

        let result = PluginManager.spawn(fu).await.unwrap();
        Result::from(result).map_err(|s| AtriError::RQError(s))
    }
}

impl Display for NamedMember {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "NamedMember({})", self.id())
    }
}

#[derive(Clone)]
pub struct AnonymousMember(ManagedCloneable);
