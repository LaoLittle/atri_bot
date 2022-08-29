use crate::loader::get_plugin_manager_vtb;
use crate::message::{Message, MessageValue};
use atri_ffi::Managed;

pub struct Image(pub(crate) Managed);

impl Image {
    pub fn id(&self) -> &str {
        let rs = (get_plugin_manager_vtb().image_get_id)(self.0.pointer);
        rs.as_str()
    }

    pub fn url(&self) -> String {
        let rs = (get_plugin_manager_vtb().image_get_url)(self.0.pointer);
        rs.into()
    }
}

impl Message for Image {
    fn push_to(self, v: &mut Vec<MessageValue>) {
        v.push(MessageValue::Image(self));
    }
}
