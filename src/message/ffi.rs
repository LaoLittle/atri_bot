use super::MessageChain;
use crate::message::at::At;
use crate::message::meta::{Anonymous, MessageMetadata, Reply};
use crate::message::MessageValue;
use atri_ffi::ffi::ForFFI;
use atri_ffi::message::meta::{ALL_META, ANONYMOUS_FLAG, FFIAnonymous, FFIMessageMetadata, FFIReply, NONE_META, REPLY_FLAG};
use atri_ffi::message::{FFIAt, FFIMessageChain, FFIMessageValue, MessageValueUnion};
use atri_ffi::{Managed, RawVec, RustString};
use std::mem::{ManuallyDrop, MaybeUninit};

impl ForFFI for MessageChain {
    type FFIValue = FFIMessageChain;

    fn into_ffi(self) -> Self::FFIValue {
        let meta = self.meta.into_ffi();
        let ffi: Vec<FFIMessageValue> = self.elems.into_iter().map(MessageValue::into_ffi).collect();

        let raw = RawVec::from(ffi);
        FFIMessageChain {
            meta,
            inner: raw
        }
    }

    fn from_ffi(ffi: Self::FFIValue) -> Self {
        let v = ffi.inner.into_vec();

        let values: Vec<MessageValue> = v.into_iter().map(MessageValue::from_ffi).collect();

        let meta = MessageMetadata::from_ffi(ffi.meta);

        Self {
            meta,
            elems: values
        }
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
        let ffi: Vec<FFIMessageValue> = self.elements.into_iter().map(MessageValue::into_ffi).collect();
        let ffi_chain = RawVec::from(ffi);

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

        let elems = elements.into_vec();
        let values: Vec<MessageValue> = elems.into_iter().map(MessageValue::from_ffi).collect();

        Self {
            reply_seq,
            sender,
            time,
            elements: values,
        }
    }
}

impl ForFFI for Anonymous {
    type FFIValue = FFIAnonymous;

    fn into_ffi(self) -> Self::FFIValue {
        let Self {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        } = self;

        let anon_id = RawVec::from(anon_id);
        let nick = RustString::from(nick);
        let color = RustString::from(color);

        FFIAnonymous {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        let FFIAnonymous {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        } = value;

        let anon_id = anon_id.into_vec();
        let nick = String::from(nick);
        let color = String::from(color);

        Self {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
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

impl ForFFI for MessageMetadata {
    type FFIValue = FFIMessageMetadata;

    fn into_ffi(self) -> Self::FFIValue {
        let Self { anonymous, reply } = self;
        let mut flags = NONE_META;
        let mut ffi_anonymous = MaybeUninit::uninit();
        let mut ffi_reply = MaybeUninit::uninit();

        if let Some(anonymous) = anonymous {
            flags |= ANONYMOUS_FLAG;
            ffi_anonymous.write(anonymous.into_ffi());
        }

        if let Some(reply) = reply {
            flags |= REPLY_FLAG;
            ffi_reply.write(reply.into_ffi());
        }

        FFIMessageMetadata {
            flags,
            anonymous: ffi_anonymous,
            reply: ffi_reply,
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        let FFIMessageMetadata {
            flags,
            anonymous,
            reply,
        } = value;

        let (anonymous, reply) = unsafe {
            match flags {
                NONE_META => (None, None),
                ANONYMOUS_FLAG => (Some(Anonymous::from_ffi(anonymous.assume_init())), None),
                REPLY_FLAG => (None, Some(Reply::from_ffi(reply.assume_init()))),
                ALL_META => (Some(Anonymous::from_ffi(anonymous.assume_init())),Some(Reply::from_ffi(reply.assume_init()))),
                _ => unreachable!(),
            }
        };

        Self {
            anonymous,
            reply
        }
    }
}
