use crate::message::MessageElement;
use ricq::msg::{MessageElem, PushElem};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct At {
    pub target: i64,
    pub display: String,
}

impl At {
    pub const ALL: Self = Self {
        target: 0,
        display: String::new(),
    };

    pub fn all() -> Self {
        Self::ALL
    }
}

impl From<At> for ricq::msg::elem::At {
    fn from(At { target, display }: At) -> Self {
        Self { target, display }
    }
}

impl PushElem for At {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        let At { target, display } = elem;

        let rq = ricq::msg::elem::At { target, display };
        PushElem::push_to(rq, vec);
    }
}

impl From<At> for MessageElement {
    fn from(at: At) -> Self {
        Self::At(at)
    }
}
