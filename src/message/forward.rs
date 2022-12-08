use crate::message::MessageChain;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ForwardMessage(Vec<ForwardNode>);

impl ForwardMessage {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> std::slice::Iter<ForwardNode> {
        self.0.iter()
    }

    pub fn from_message_chain(
        sender_id: i64,
        sender_name: String,
        time: i32,
        chain: MessageChain,
    ) -> Self {
        Self(vec![ForwardNode::NormalMessage {
            info: ForwardNodeInfo {
                sender_id,
                sender_name,
                time,
            },
            chain,
        }])
    }
}

impl IntoIterator for ForwardMessage {
    type Item = ForwardNode;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ForwardNode {
    NormalMessage {
        #[serde(flatten)]
        info: ForwardNodeInfo,
        chain: MessageChain,
    },
    ForwardMessage {
        #[serde(flatten)]
        info: ForwardNodeInfo,
        forward: ForwardMessage,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ForwardNodeInfo {
    pub sender_id: i64,
    pub sender_name: String,
    pub time: i32,
}

impl<const N: usize> From<[ForwardNode; N]> for ForwardMessage {
    fn from(value: [ForwardNode; N]) -> Self {
        Self(value.to_vec())
    }
}

impl From<Vec<ricq::structs::ForwardMessage>> for ForwardMessage {
    fn from(value: Vec<ricq::structs::ForwardMessage>) -> Self {
        let mut nodes: Vec<ForwardNode> = Vec::with_capacity(value.len());

        for msg in value {
            nodes.push(match msg {
                ricq::structs::ForwardMessage::Message(node) => ForwardNode::NormalMessage {
                    info: ForwardNodeInfo {
                        sender_id: node.sender_id,
                        sender_name: node.sender_name,
                        time: node.time,
                    },
                    chain: node.elements.into(),
                },
                ricq::structs::ForwardMessage::Forward(node) => ForwardNode::ForwardMessage {
                    info: ForwardNodeInfo {
                        sender_id: node.sender_id,
                        sender_name: node.sender_name,
                        time: node.time,
                    },
                    forward: node.nodes.into(),
                },
            });
        }

        Self(nodes)
    }
}

impl From<ForwardMessage> for Vec<ricq::structs::ForwardMessage> {
    fn from(value: ForwardMessage) -> Self {
        let mut nodes = Self::with_capacity(value.len());

        for node in value.0 {
            nodes.push(match node {
                ForwardNode::NormalMessage { info, chain } => {
                    ricq::structs::ForwardMessage::Message(ricq::structs::MessageNode {
                        sender_id: info.sender_id,
                        time: info.time,
                        sender_name: info.sender_name,
                        elements: chain.into(),
                    })
                }
                ForwardNode::ForwardMessage { info, forward: msg } => {
                    ricq::structs::ForwardMessage::Forward(ricq::structs::ForwardNode {
                        sender_id: info.sender_id,
                        time: info.time,
                        sender_name: info.sender_name,
                        nodes: msg.into(),
                    })
                }
            });
        }

        nodes
    }
}

mod ffi {
    use super::{ForwardMessage, ForwardNode, ForwardNodeInfo};
    use crate::message::MessageChain;
    use atri_ffi::ffi::ForFFI;
    use atri_ffi::message::forward::{FFIForwardNode, FFIForwardNodeInfo, ForwardNodeUnion};
    use atri_ffi::RustVec;
    use std::mem::ManuallyDrop;

    impl ForFFI for ForwardNode {
        type FFIValue = FFIForwardNode;

        fn into_ffi(self) -> Self::FFIValue {
            match self {
                Self::NormalMessage { info, chain } => FFIForwardNode {
                    is_normal: true,
                    info: info.into_ffi(),
                    inner: ForwardNodeUnion {
                        normal: ManuallyDrop::new(chain.into_ffi()),
                    },
                },
                Self::ForwardMessage { info, forward } => FFIForwardNode {
                    is_normal: false,
                    info: info.into_ffi(),
                    inner: ForwardNodeUnion {
                        forward: ManuallyDrop::new(forward.into_ffi()),
                    },
                },
            }
        }

        fn from_ffi(
            FFIForwardNode {
                is_normal,
                info,
                inner,
            }: Self::FFIValue,
        ) -> Self {
            unsafe {
                if is_normal {
                    Self::NormalMessage {
                        info: ForwardNodeInfo::from_ffi(info),
                        chain: MessageChain::from_ffi(ManuallyDrop::into_inner(inner.normal)),
                    }
                } else {
                    Self::ForwardMessage {
                        info: ForwardNodeInfo::from_ffi(info),
                        forward: ForwardMessage::from_ffi(ManuallyDrop::into_inner(inner.forward)),
                    }
                }
            }
        }
    }

    impl ForFFI for ForwardNodeInfo {
        type FFIValue = FFIForwardNodeInfo;

        fn into_ffi(self) -> Self::FFIValue {
            let ForwardNodeInfo {
                sender_id,
                sender_name,
                time,
            } = self;

            FFIForwardNodeInfo {
                sender_id,
                sender_name: sender_name.into(),
                time,
            }
        }

        fn from_ffi(
            FFIForwardNodeInfo {
                sender_id,
                sender_name,
                time,
            }: Self::FFIValue,
        ) -> Self {
            Self {
                sender_id,
                sender_name: sender_name.into(),
                time,
            }
        }
    }

    impl ForFFI for ForwardMessage {
        type FFIValue = RustVec<FFIForwardNode>;

        fn into_ffi(self) -> Self::FFIValue {
            self.0
                .into_iter()
                .map(ForwardNode::into_ffi)
                .collect::<Vec<FFIForwardNode>>()
                .into()
        }

        fn from_ffi(rs: Self::FFIValue) -> Self {
            Self(
                rs.into_vec()
                    .into_iter()
                    .map(ForwardNode::from_ffi)
                    .collect(),
            )
        }
    }
}
