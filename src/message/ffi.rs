use super::MessageChain;
use crate::message::at::At;
use crate::message::meta::{Anonymous, MessageMetadata, Reply};
use crate::message::MessageElement;
use atri_ffi::ffi::ForFFI;
use atri_ffi::message::meta::{
    FFIAnonymous, FFIMessageMetadata, FFIReply, ANONYMOUS_FLAG, NONE_META, REPLY_FLAG,
};
use atri_ffi::message::{
    FFIAt, FFIMessageChain, FFIMessageValue, MessageValueFlag, MessageValueUnion,
};
use atri_ffi::{Managed, RustString, RustVec};
use std::mem::{ManuallyDrop, MaybeUninit};

impl ForFFI for MessageChain {
    type FFIValue = FFIMessageChain;

    fn into_ffi(self) -> Self::FFIValue {
        let meta = self.meta.into_ffi();
        let ffi: Vec<FFIMessageValue> = self
            .elements
            .into_iter()
            .map(MessageElement::into_ffi)
            .collect();

        let raw = RustVec::from(ffi);
        FFIMessageChain { meta, inner: raw }
    }

    fn from_ffi(ffi: Self::FFIValue) -> Self {
        let meta = MessageMetadata::from_ffi(ffi.meta);

        let v = ffi.inner.into_vec();
        let values: Vec<MessageElement> = v.into_iter().map(MessageElement::from_ffi).collect();

        Self {
            meta,
            elements: values,
        }
    }
}

impl ForFFI for MessageElement {
    type FFIValue = FFIMessageValue;

    fn into_ffi(self) -> Self::FFIValue {
        match self {
            MessageElement::Text(s) => FFIMessageValue {
                t: MessageValueFlag::Text.value(),
                union: MessageValueUnion {
                    text: ManuallyDrop::new(RustString::from(s)),
                },
            },
            MessageElement::Image(img) => FFIMessageValue {
                t: MessageValueFlag::Image.value(),
                union: MessageValueUnion {
                    image: ManuallyDrop::new(Managed::from_value(img)),
                },
            },
            MessageElement::At(At { target, display }) => FFIMessageValue {
                t: MessageValueFlag::At.value(),
                union: MessageValueUnion {
                    at: ManuallyDrop::new({
                        FFIAt {
                            target,
                            display: RustString::from(display),
                        }
                    }),
                },
            },
            MessageElement::AtAll => FFIMessageValue {
                t: MessageValueFlag::AtAll.value(),
                union: MessageValueUnion { at_all: () },
            },
            or => FFIMessageValue {
                t: MessageValueFlag::Unknown.value(),
                union: MessageValueUnion {
                    unknown: ManuallyDrop::new(Managed::from_value(or)),
                },
            },
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        unsafe {
            match MessageValueFlag::try_from(value.t)
                .unwrap_or_else(|e| panic!("Unknown message value flag: {}", e))
            {
                MessageValueFlag::Text => {
                    MessageElement::Text(ManuallyDrop::into_inner(value.union.text).into())
                }
                MessageValueFlag::Image => {
                    MessageElement::Image(ManuallyDrop::into_inner(value.union.image).into_value())
                }
                MessageValueFlag::At => {
                    let inner = ManuallyDrop::into_inner(value.union.at);
                    MessageElement::At(At {
                        target: inner.target,
                        display: String::from(inner.display),
                    })
                }
                MessageValueFlag::AtAll => MessageElement::AtAll,
                MessageValueFlag::Unknown => {
                    ManuallyDrop::into_inner(value.union.unknown).into_value()
                }
            }
        }
    }
}

impl ForFFI for Reply {
    type FFIValue = FFIReply;

    fn into_ffi(self) -> Self::FFIValue {
        let ffi: Vec<FFIMessageValue> = self
            .elements
            .into_iter()
            .map(MessageElement::into_ffi)
            .collect();
        let ffi_chain = RustVec::from(ffi);

        FFIReply {
            reply_seq: self.reply_seq,
            sender: self.sender,
            time: self.time,
            elements: ffi_chain,
        }
    }

    fn from_ffi(FFIReply {
                    reply_seq,
                    sender,
                    time,
                    elements,
                }: Self::FFIValue) -> Self {
        let elems = elements.into_vec();
        let values: Vec<MessageElement> = elems.into_iter().map(MessageElement::from_ffi).collect();

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

        let anon_id = RustVec::from(anon_id);
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

    fn from_ffi(FFIAnonymous {
                    anon_id,
                    nick,
                    portrait_index,
                    bubble_index,
                    expire_time,
                    color,
                }: Self::FFIValue) -> Self {
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
        let Self { target, display } = self;

        FFIAt {
            target,
            display: RustString::from(display),
        }
    }

    fn from_ffi(FFIAt { target, display }: Self::FFIValue) -> Self {
        Self {
            target,
            display: String::from(display),
        }
    }
}

impl ForFFI for MessageMetadata {
    type FFIValue = FFIMessageMetadata;

    fn into_ffi(self) -> Self::FFIValue {
        let Self {
            seqs,
            rands,
            time,
            sender,
            anonymous,
            reply,
        } = self;
        let mut flags = NONE_META;

        let mut ffi_anonymous = MaybeUninit::uninit();
        if let Some(anonymous) = anonymous {
            flags |= ANONYMOUS_FLAG;
            ffi_anonymous.write(anonymous.into_ffi());
        }

        let mut ffi_reply = MaybeUninit::uninit();
        if let Some(reply) = reply {
            flags |= REPLY_FLAG;
            ffi_reply.write(reply.into_ffi());
        }

        FFIMessageMetadata {
            seqs: seqs.into(),
            rands: rands.into(),
            time,
            sender,
            flags,
            anonymous: ffi_anonymous,
            reply: ffi_reply,
        }
    }

    fn from_ffi(FFIMessageMetadata {
                    seqs,
                    rands,
                    time,
                    sender,
                    flags,
                    anonymous,
                    reply,
                }: Self::FFIValue) -> Self {
        unsafe {
            Self {
                seqs: seqs.into_vec(),
                rands: rands.into_vec(),
                time,
                sender,
                anonymous: if flags & ANONYMOUS_FLAG != 0 {
                    Some(Anonymous::from_ffi(anonymous.assume_init()))
                } else {
                    None
                },
                reply: if flags & REPLY_FLAG != 0 {
                    Some(Reply::from_ffi(reply.assume_init()))
                } else {
                    None
                },
            }
        }
    }
}
