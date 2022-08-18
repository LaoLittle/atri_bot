use super::MessageChain;
use crate::message::at::At;
use crate::message::reply::Reply;
use crate::message::MessageValue;
use atri_ffi::ffi::ForFFI;
use atri_ffi::message::{FFIAt, FFIMessageChain, FFIMessageValue, FFIReply, MessageValueUnion};
use atri_ffi::{Managed, RawVec, RustString};
use std::mem::ManuallyDrop;

impl ForFFI for MessageChain {
    type FFIValue = FFIMessageChain;

    fn into_ffi(self) -> Self::FFIValue {
        let ffi: Vec<FFIMessageValue> = self.0.into_iter().map(MessageValue::into_ffi).collect();

        let raw = RawVec::from(ffi);
        FFIMessageChain { inner: raw }
    }

    fn from_ffi(ffi: Self::FFIValue) -> Self {
        let v = ffi.inner.into_vec();

        let values: Vec<MessageValue> = v.into_iter().map(MessageValue::from_ffi).collect();
        Self(values)
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
                    image: ManuallyDrop::new(Managed::from_value(img)),
                },
            },
            MessageValue::Reply(reply) => FFIMessageValue {
                t: 2,
                union: MessageValueUnion {
                    reply: ManuallyDrop::new(reply.into_ffi()),
                },
            },
            MessageValue::At(At { target, display }) => FFIMessageValue {
                t: 3,
                union: MessageValueUnion {
                    at: ManuallyDrop::new({
                        FFIAt {
                            target,
                            display: RustString::from(display),
                        }
                    }),
                },
            },
            or => FFIMessageValue {
                t: 255,
                union: MessageValueUnion {
                    unknown: ManuallyDrop::new(Managed::from_value(or)),
                },
            },
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        unsafe {
            match value.t {
                0 => MessageValue::Text(ManuallyDrop::into_inner(value.union.text).into()),
                1 => MessageValue::Image(ManuallyDrop::into_inner(value.union.image).into_value()),
                2 => MessageValue::Reply(Reply::from_ffi(ManuallyDrop::into_inner(
                    value.union.reply,
                ))),
                3 => {
                    let inner = ManuallyDrop::into_inner(value.union.at);
                    MessageValue::At(At {
                        target: inner.target,
                        display: String::from(inner.display),
                    })
                }
                _ => ManuallyDrop::into_inner(value.union.unknown).into_value(),
            }
        }
    }
}

impl ForFFI for Reply {
    type FFIValue = FFIReply;

    fn into_ffi(self) -> Self::FFIValue {
        let ffi_chain = self.elements.into_ffi();

        FFIReply {
            reply_seq: self.reply_seq,
            sender: self.sender,
            time: self.time,
            elements: ffi_chain,
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
            elements: MessageChain::from_ffi(elements),
        }
    }
}

impl ForFFI for At {
    type FFIValue = FFIAt;

    fn into_ffi(self) -> Self::FFIValue {
        let At { target, display } = self;

        FFIAt {
            target,
            display: RustString::from(display),
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        let FFIAt { target, display } = value;

        Self {
            target,
            display: String::from(display),
        }
    }
}
