use atri_ffi::{Managed};
use atri_ffi::message::FFIMessageChain;
use crate::error::AtriError;
use crate::loader::get_plugin_manager_vtb;
use crate::message::{Image, MessageChain, MessageReceipt};

pub struct Group(pub(crate) Managed);

impl Group {
    pub async fn send_message(&self, chain: MessageChain) -> Result<MessageReceipt, AtriError> {
        let fu = {
            let ffi: FFIMessageChain = chain.into_ffi();
            (get_plugin_manager_vtb().group_send_message)(self.0.pointer, ffi)
        };

        let res = fu.await;
        match Result::from(res) {
            Ok(ma) => Ok(MessageReceipt::from_managed(ma)),
            Err(s) => Err(AtriError::RQError(s))
        }
    }

    pub async fn upload_image(&self, image: Vec<u8>) -> Result<Image, AtriError> {
        let fu = {
            (get_plugin_manager_vtb().group_upload_image)(self.0.pointer, image.into())
        };

        let result = fu.await;
        match Result::from(result) {
            Ok(ma) => Ok(Image::from_managed(ma)),
            Err(e) => Err(AtriError::RQError(e))
        }
    }
}