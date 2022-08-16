use atri_ffi::Managed;
use atri_ffi::message::FFIMessageChain;
use crate::loader::get_plugin_manager_vtb;
use crate::message::MessageChain;

pub struct Group(pub(crate) Managed);

impl Group {
    pub async fn send_message(&self, chain: MessageChain) {
        let fu = {
            let ffi: FFIMessageChain = chain.into_ffi();
            (get_plugin_manager_vtb().group_send_message)(self.0.pointer, ffi)
        };
        fu.await;
    }
}