use ricq::msg::elem::{FriendImage, GroupImage};
use ricq::msg::{MessageElem, PushElem};

#[derive(Clone)]
pub enum Image {
    Group(GroupImage),
    Friend(FriendImage),
}

impl PushElem for Image {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        match elem {
            Self::Group(img) => PushElem::push_to(img, vec),
            Self::Friend(img) => PushElem::push_to(img, vec),
        }
    }
}
