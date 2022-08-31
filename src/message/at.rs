use crate::message::MessageValue;
use ricq::msg::{MessageElem, PushElem};

#[derive(Clone)]
pub struct At {
    pub target: i64,
    pub display: String,
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

impl From<At> for MessageValue {
    fn from(at: At) -> Self {
        Self::At(at)
    }
}
