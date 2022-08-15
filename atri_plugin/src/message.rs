use atri_ffi::Managed;
use crate::loader::get_plugin_manager_vtb;

pub struct MessageChain(pub(crate) Managed);

impl MessageChain {
    
}

impl ToString for MessageChain {
    fn to_string(&self) -> String {
        let rs = (get_plugin_manager_vtb().message_chain_to_string)(self.0.pointer);
        String::from(rs)
    }
}

pub struct MessageChainIter(pub(crate) Managed);