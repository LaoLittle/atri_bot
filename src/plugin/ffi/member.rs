use super::cast_ref;
use super::rt::future_block_on;
use crate::contact::member::NamedMember;
use crate::plugin::ffi::group::group_to_handle;
use atri_ffi::error::FFIResult;
use atri_ffi::future::FFIFuture;
use atri_ffi::{Handle, RustStr};

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

pub extern "C" fn named_member_get_group(named: *const ()) -> Handle {
    let named: &NamedMember = cast_ref(named);
    unsafe { group_to_handle(named.group()) }
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

pub extern "C" fn named_member_change_card_name_blocking(
    manager: *const (),
    named: *const (),
    card: RustStr,
) -> FFIResult<()> {
    let named: &NamedMember = cast_ref(named);
    let card = card.as_ref().to_owned();

    future_block_on(manager, async move {
        let result = named.change_card_name(card).await;
        FFIResult::from(result)
    })
}
