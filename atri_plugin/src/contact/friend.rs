use std::slice;
use atri_ffi::Managed;
use crate::bot::Bot;
use crate::error::AtriError;
use crate::loader::get_plugin_manager_vtb;
use crate::message::{MessageChain, MessageReceipt};

pub struct Friend(pub(crate) Managed);

impl Friend {
    pub fn id(&self) -> i64 {
        (get_plugin_manager_vtb().friend_get_id)(self.0.pointer)
    }

    pub fn nickname(&self) -> &str{
        let rstr = (get_plugin_manager_vtb().friend_get_nickname)(self.0.pointer);

        unsafe {
            let slice = slice::from_raw_parts(rstr.slice, rstr.len);
            std::str::from_utf8_unchecked(slice)
        }
    }

    pub fn bot(&self) -> Bot {
        let ma = (get_plugin_manager_vtb().friend_get_bot)(self.0.pointer);
        Bot(ma)
    }

    pub async fn send_message(&self, chain: MessageChain) -> Result<MessageReceipt, AtriError> {
        let fu = {
            let ffi = chain.into_ffi();
            (get_plugin_manager_vtb().friend_send_message)(self.0.pointer, ffi)
        };

        let result = Result::from(fu.await);
        match result {
            Ok(ma) => Ok(MessageReceipt(ma)),
            Err(s) => Err(AtriError::RQError(s))
        }
    }
}