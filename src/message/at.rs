use ricq::msg::{MessageElem, PushElem};

pub struct At {
    pub target: i64,
    pub display: String,
}

impl PushElem for At {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        let At { target, display } = elem;

        let rq = ricq::msg::elem::At { target, display };
        PushElem::push_to(rq, vec);
    }
}
