use crate::bot::Bot;
use crate::error::AtriError;
use crate::loader::get_plugin_manager_vtb;
use crate::message::{Image, MessageChain, MessageReceipt};
use atri_ffi::ffi::ForFFI;
use atri_ffi::message::FFIMessageChain;
use atri_ffi::ManagedCloneable;
use std::fmt::{Display, Formatter};

#[derive(Clone)]
pub struct Group(pub(crate) ManagedCloneable);

impl Group {
    pub fn id(&self) -> i64 {
        (get_plugin_manager_vtb().group_get_id)(self.0.pointer)
    }

    pub fn bot(&self) -> Bot {
        let ma = (get_plugin_manager_vtb().group_get_bot)(self.0.pointer);
        Bot(ma)
    }

    pub fn name(&self) -> &str {
        let rs = (get_plugin_manager_vtb().group_get_name)(self.0.pointer);
        // Safety: this slice should live as long as self(Group)
        rs.as_str()
    }

    pub async fn send_message(&self, chain: MessageChain) -> Result<MessageReceipt, AtriError> {
        let fu = {
            let ffi: FFIMessageChain = chain.into_ffi();
            (get_plugin_manager_vtb().group_send_message)(self.0.pointer, ffi)
        };

        let res = fu.await;
        match Result::from(res) {
            Ok(ma) => Ok(MessageReceipt(ma)),
            Err(s) => Err(AtriError::RQError(s)),
        }
    }

    pub async fn upload_image(&self, image: Vec<u8>) -> Result<Image, AtriError> {
        let fu = { (get_plugin_manager_vtb().group_upload_image)(self.0.pointer, image.into()) };

        let result = fu.await;
        match Result::from(result) {
            Ok(ma) => Ok(Image(ma)),
            Err(e) => Err(AtriError::RQError(e)),
        }
    }

    pub async fn quit(&self) {
        (get_plugin_manager_vtb().group_quit)(self.0.pointer).await;
    }
}

impl Display for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Group({})", self.id())
    }
}
