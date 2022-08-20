use crate::bot::Bot;
use atri_ffi::contact::FFIMember;
use atri_ffi::{ManagedCloneable, RustStr, RustString};
use std::mem::ManuallyDrop;
use std::slice;

use crate::contact::group::Group;
use crate::error::AtriError;
use crate::loader::get_plugin_manager_vtb;

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
pub struct NamedMember(ManagedCloneable);

impl NamedMember {
    pub fn id(&self) -> i64 {
        (get_plugin_manager_vtb().named_member_get_id)(self.0.pointer)
    }

    pub fn nickname(&self) -> &str {
        let RustStr { slice, len } =
            (get_plugin_manager_vtb().named_member_get_nickname)(self.0.pointer);

        unsafe {
            let slice = slice::from_raw_parts(slice, len);
            std::str::from_utf8_unchecked(slice)
        }
    }

    pub fn card_name(&self) -> &str {
        let RustStr { slice, len } =
            (get_plugin_manager_vtb().named_member_get_card_name)(self.0.pointer);

        unsafe {
            let slice = slice::from_raw_parts(slice, len);
            std::str::from_utf8_unchecked(slice)
        }
    }

    pub fn group(&self) -> Group {
        let ma = (get_plugin_manager_vtb().named_member_get_group)(self.0.pointer);
        Group(ma)
    }

    pub fn bot(&self) -> Bot {
        self.group().bot()
    }

    pub async fn change_card_name<S: ToString>(&self, card_name: S) -> Result<(), AtriError> {
        let str = card_name.to_string();
        let rs = RustString::from(str);

        let result =
            (get_plugin_manager_vtb().named_member_change_card_name)(self.0.pointer, rs).await;
        Result::from(result).map_err(|s| AtriError::RQError(s))
    }
}

#[derive(Clone)]
pub struct AnonymousMember(ManagedCloneable);
