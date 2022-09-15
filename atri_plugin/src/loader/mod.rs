use atri_ffi::closure::FFIFn;
use atri_ffi::contact::FFIMember;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::{AtriManager, FFIEvent};
use atri_ffi::future::FFIFuture;
use atri_ffi::message::FFIMessageChain;
use atri_ffi::{Managed, ManagedCloneable, RustStr, RustString, RustVec};
use std::mem::MaybeUninit;

pub struct AtriVTable {
    pub plugin_manager_spawn:
        extern "C" fn(manager: *const (), FFIFuture<Managed>) -> FFIFuture<FFIResult<Managed>>,
    pub plugin_manager_block_on: extern "C" fn(manager: *const (), FFIFuture<Managed>) -> Managed,

    pub new_listener: extern "C" fn(FFIFn<FFIEvent, FFIFuture<bool>>) -> Managed,

    pub event_intercept: extern "C" fn(intercepted: *const ()),
    pub event_is_intercepted: extern "C" fn(intercepted: *const ()) -> bool,

    pub bot_get_id: extern "C" fn(bot: *const ()) -> i64,
    pub bot_get_nickname: extern "C" fn(bot: *const ()) -> RustString,
    pub bot_get_list: extern "C" fn() -> RustVec<ManagedCloneable>,
    pub find_bot: extern "C" fn(id: i64) -> ManagedCloneable,
    pub bot_find_group: extern "C" fn(bot: *const (), id: i64) -> ManagedCloneable,
    pub bot_find_friend: extern "C" fn(bot: *const (), id: i64) -> ManagedCloneable,
    pub bot_get_groups: extern "C" fn(bot: *const ()) -> RustVec<ManagedCloneable>,
    pub bot_get_friends: extern "C" fn(bot: *const ()) -> RustVec<ManagedCloneable>,

    pub group_message_event_get_group: extern "C" fn(event: *const ()) -> ManagedCloneable,
    pub group_message_event_get_message: extern "C" fn(event: *const ()) -> FFIMessageChain,
    pub group_message_event_get_sender: extern "C" fn(event: *const ()) -> FFIMember,

    pub group_get_id: extern "C" fn(group: *const ()) -> i64,
    pub group_get_name: extern "C" fn(group: *const ()) -> RustStr,
    pub group_get_bot: extern "C" fn(group: *const ()) -> ManagedCloneable,
    pub group_get_members: extern "C" fn(group: *const ()) -> FFIFuture<RustVec<ManagedCloneable>>,
    pub group_find_member: extern "C" fn(group: *const (), id: i64) -> ManagedCloneable,
    pub group_get_named_member:
        extern "C" fn(group: *const (), id: i64) -> FFIFuture<ManagedCloneable>,
    pub group_send_message:
        extern "C" fn(group: *const (), chain: FFIMessageChain) -> FFIFuture<FFIResult<Managed>>,
    pub group_upload_image:
        extern "C" fn(group: *const (), data: RustVec<u8>) -> FFIFuture<FFIResult<Managed>>,
    pub group_quit: extern "C" fn(group: *const ()) -> FFIFuture<bool>,
    pub group_change_name:
        extern "C" fn(group: *const (), name: RustString) -> FFIFuture<FFIResult<()>>,

    pub friend_message_event_get_friend: extern "C" fn(event: *const ()) -> ManagedCloneable,
    pub friend_message_event_get_message: extern "C" fn(event: *const ()) -> FFIMessageChain,
    pub friend_get_id: extern "C" fn(friend: *const ()) -> i64,
    pub friend_get_nickname: extern "C" fn(friend: *const ()) -> RustStr,
    pub friend_get_bot: extern "C" fn(friend: *const ()) -> ManagedCloneable,
    pub friend_send_message:
        extern "C" fn(friend: *const (), chain: FFIMessageChain) -> FFIFuture<FFIResult<Managed>>,
    pub friend_upload_image:
        extern "C" fn(friend: *const (), img: RustVec<u8>) -> FFIFuture<FFIResult<Managed>>,

    pub named_member_get_id: extern "C" fn(named: *const ()) -> i64,
    pub named_member_get_nickname: extern "C" fn(named: *const ()) -> RustStr,
    pub named_member_get_card_name: extern "C" fn(named: *const ()) -> RustStr,
    pub named_member_get_group: extern "C" fn(named: *const ()) -> ManagedCloneable,
    pub named_member_change_card_name:
        extern "C" fn(named: *const (), card: RustString) -> FFIFuture<FFIResult<()>>,

    pub image_get_id: extern "C" fn(img: *const ()) -> RustStr,
    // flash
    pub image_get_url: extern "C" fn(img: *const ()) -> RustString,

    pub log: extern "C" fn(handle: usize, manager: *const (), level: u8, log: RustString),
}

static mut ATRI_MANAGER: MaybeUninit<AtriManager> = MaybeUninit::uninit();

static mut ATRI_VTABLE: MaybeUninit<AtriVTable> = MaybeUninit::uninit();

/// Safety: This function will be called by the plugin manager first
#[no_mangle]
unsafe extern "C" fn atri_manager_init(manager: AtriManager) {
    let get_fun = manager.get_fun;

    ATRI_MANAGER.write(manager);

    macro_rules! vtb {
        (get_fun: $fun:expr; $($field:ident => $sig:expr),* $(,)?) => {
            AtriVTable {
                $($field: std::mem::transmute(($fun)($sig)),)*
            }
        };
    }

    let vtable = vtb! {
        get_fun: get_fun;
        plugin_manager_spawn => 0,
        plugin_manager_block_on => 1,

        new_listener => 100,

        event_intercept => 200,
        event_is_intercepted => 201,

        bot_get_id => 300,
        bot_get_nickname => 301,
        bot_get_list => 302,
        find_bot => 303,
        bot_find_group => 304,
        bot_find_friend => 305,
        bot_get_groups => 306,
        bot_get_friends => 307,

        group_get_id => 400,
        group_get_name => 401,
        group_get_bot => 402,
        group_get_members => 403,
        group_find_member => 404,
        group_get_named_member => 405,
        group_send_message => 406,
        group_upload_image => 407,
        group_quit => 408,
        group_change_name => 409,

        friend_get_id => 500,
        friend_get_nickname => 501,
        friend_get_bot => 502,
        friend_send_message => 503,
        friend_upload_image => 504,

        named_member_get_id => 600,
        named_member_get_nickname => 601,
        named_member_get_card_name => 602,
        named_member_get_group => 603,
        named_member_change_card_name => 604,

        group_message_event_get_group => 10000,
        group_message_event_get_message => 10001,
        group_message_event_get_sender => 10002,

        friend_message_event_get_friend => 10100,
        friend_message_event_get_message => 10101,

        image_get_id => 2000,
        // flash => 2001
        image_get_url => 2002,

        log => 20000,
    };

    ATRI_VTABLE.write(vtable);
}

fn get_atri_manager() -> &'static AtriManager {
    unsafe { ATRI_MANAGER.assume_init_ref() }
}

pub(crate) fn get_plugin_manager() -> *const () {
    get_atri_manager().manager_ptr
}

pub(crate) fn get_plugin_handle() -> usize {
    get_atri_manager().handle
}

pub(crate) fn get_plugin_manager_vtb() -> &'static AtriVTable {
    unsafe { ATRI_VTABLE.assume_init_ref() }
}
