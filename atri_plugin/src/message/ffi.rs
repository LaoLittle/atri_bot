use std::mem::ManuallyDrop;
use atri_ffi::ffi::ForFFI;
use atri_ffi::message::{FFIAt, FFIMessageChain, FFIMessageValue, FFIReply, MessageValueUnion};
use atri_ffi::{RawVec, RustString};
use crate::message::{Image, MessageChain, MessageValue};
use crate::message::at::At;
use crate::message::reply::Reply;

impl ForFFI for MessageChain {
    type FFIValue = FFIMessageChain;

    fn into_ffi(self) -> Self::FFIValue {
        let v: Vec<FFIMessageValue> = self.0.into_iter().map(MessageValue::into_ffi).collect();

        let raw = RawVec::from(v);
        FFIMessageChain { inner: raw }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        let v = value.inner.into_vec();
        Self(v.into_iter().map(MessageValue::from_ffi).collect())
    }
}

impl ForFFI for MessageValue {
    type FFIValue = FFIMessageValue;

    fn into_ffi(self) -> Self::FFIValue {
        match self {
            MessageValue::Text(s) => FFIMessageValue {
                t: 0,
                union: MessageValueUnion {
                    text: ManuallyDrop::new(RustString::from(s)),
                },
            },
            MessageValue::Image(img) => FFIMessageValue {
                t: 1,
                union: MessageValueUnion {
                    image: ManuallyDrop::new(img.0),
                },
            },
            MessageValue::Reply(reply) => FFIMessageValue{
                t: 2,
                union: MessageValueUnion {
                    reply: ManuallyDrop::new(reply.into_ffi())
                },
            },
            MessageValue::At(At {
                                 target, display
                             }) => FFIMessageValue {
                t: 3,
                union: MessageValueUnion {
                    at: ManuallyDrop::new({
                        let display = RustString::from(display);
                        FFIAt {
                            target,
                            display
                        }
                    })
                }
            },
            MessageValue::Unknown(ma) => FFIMessageValue {
                t: 255,
                union: MessageValueUnion {
                    unknown: ManuallyDrop::new(ma),
                },
            },
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        unsafe {
            match value.t {
                0 => Self::Text(ManuallyDrop::into_inner(value.union.text).into()),
                1 => Self::Image(Image(ManuallyDrop::into_inner(value.union.image))),
                2 => Self::Reply(Reply::from_ffi(ManuallyDrop::into_inner(value.union.reply))),
                3 => {
                    let FFIAt {
                        target,display
                    } = ManuallyDrop::into_inner(value.union.at);
                    let display = String::from(display);

                    Self::At(At {
                        target,display
                    })
                }
                _ => Self::Unknown(ManuallyDrop::into_inner(value.union.unknown)),
            }
        }
    }
}

impl ForFFI for Reply {
    type FFIValue = FFIReply;

    fn into_ffi(self) -> Self::FFIValue {
        let Self {
            reply_seq,
            sender,
            time,
            elements
        } = self;

        FFIReply {
            reply_seq,
            sender,
            time,
            elements: elements.into_ffi()
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        let FFIReply {
            reply_seq,
            sender,
            time,
            elements,
        } = value;

        Self {
            reply_seq,
            sender,
            time,
            elements: MessageChain::from_ffi(elements)
        }
    }
}

impl ForFFI for At {
    type FFIValue = FFIAt;

    fn into_ffi(self) -> Self::FFIValue {
        let At {
            target,display
        } = self;

        FFIAt {
            target,
            display: RustString::from(display)
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        let FFIAt {
            target,display
        } = value;

        Self {
            target,
            display: String::from(display)
        }
    }
}