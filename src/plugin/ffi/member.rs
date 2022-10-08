use crate::contact::member::NamedMember;
use crate::plugin::cast_ref;
use atri_ffi::error::FFIResult;
use atri_ffi::future::FFIFuture;
use atri_ffi::{ManagedCloneable, RustStr};

pub extern "C" fn named_member_get_id(named: *const ()) -> i64 {
    let named: &NamedMember = cast_ref(named);
    named.id()
}

pub extern "C" fn named_member_get_nickname(named: *const ()) -> RustStr {
    let named: &NamedMember = cast_ref(named);
    RustStr::from(named.nickname())
}

pub extern "C" fn named_member_get_card_name(named: *const ()) -> RustStr {
    let named: &NamedMember = cast_ref(named);
    RustStr::from(named.card_name())
}

pub extern "C" fn named_member_get_group(named: *const ()) -> ManagedCloneable {
    let named: &NamedMember = cast_ref(named);
    let g = named.group().clone();
    ManagedCloneable::from_value(g)
}

pub extern "C" fn named_member_change_card_name(
    named: *const (),
    card: RustStr,
) -> FFIFuture<FFIResult<()>> {
    let card = card.as_ref().to_owned();
    FFIFuture::from(async move {
        let named: &NamedMember = cast_ref(named);
        let result = named.change_card_name(card).await;
        FFIResult::from(result)
    })
}
