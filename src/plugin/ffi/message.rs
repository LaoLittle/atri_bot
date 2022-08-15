use crate::plugin::cast_ref;
use crate::MessageChain;
use atri_ffi::RustString;

pub extern "C" fn message_chain_to_string(chain: *const ()) -> RustString {
    let chain: &MessageChain = cast_ref(chain);
    let rstring = RustString::from(chain.to_string());
    rstring
}
