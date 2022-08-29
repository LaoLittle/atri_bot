use crate::message::MessageValue;
use ricq::msg::elem::{FlashImage, FriendImage, GroupImage};
use ricq::msg::{MessageElem, PushElem};

#[derive(Clone)]
pub enum Image {
    Group(GroupImage),
    Friend(FriendImage),
}

impl Image {
    pub fn id(&self) -> &str {
        match self {
            Self::Group(g) => &g.file_path,
            Self::Friend(f) => &f.file_path,
        }
    }

    pub fn flash(self) -> FlashImage {
        match self {
            Self::Group(g) => g.flash(),
            Self::Friend(f) => f.flash(),
        }
    }

    pub fn url(&self) -> String {
        match self {
            Self::Group(g) => g.url(),
            Self::Friend(f) => f.url(),
        }
    }
}

impl PushElem for Image {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        match elem {
            Self::Group(img) => PushElem::push_to(img, vec),
            Self::Friend(img) => PushElem::push_to(img, vec),
        }
    }
}

impl From<GroupImage> for Image {
    fn from(g: GroupImage) -> Self {
        Self::Group(g)
    }
}

impl From<FriendImage> for Image {
    fn from(f: FriendImage) -> Self {
        Self::Friend(f)
    }
}

impl From<Image> for MessageValue {
    fn from(img: Image) -> Self {
        Self::Image(img)
    }
}
