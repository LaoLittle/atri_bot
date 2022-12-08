use ricq::msg::{MessageElem, PushElem};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Face {
    pub index: i32,
    pub name: String,
}

impl From<ricq::msg::elem::Face> for Face {
    fn from(ricq::msg::elem::Face { index, name }: ricq::msg::elem::Face) -> Self {
        Self { index, name }
    }
}

impl From<Face> for ricq::msg::elem::Face {
    fn from(Face { index, name }: Face) -> Self {
        Self { index, name }
    }
}

impl PushElem for Face {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        let rq: ricq::msg::elem::Face = elem.into();
        PushElem::push_to(rq, vec);
    }
}

mod ffi {
    use crate::message::face::Face;
    use atri_ffi::ffi::ForFFI;
    use atri_ffi::message::FFIFace;

    impl ForFFI for Face {
        type FFIValue = FFIFace;

        fn into_ffi(self) -> Self::FFIValue {
            let Face { index, name } = self;

            FFIFace {
                index,
                name: name.into(),
            }
        }

        fn from_ffi(FFIFace { index, name }: Self::FFIValue) -> Self {
            Self {
                index,
                name: name.into(),
            }
        }
    }
}
