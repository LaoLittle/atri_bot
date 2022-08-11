use crate::contact::group::Group;
use crate::GroupMemberInfo;
use std::sync::Arc;

#[derive(Clone)]
pub struct NamedMember(Arc<imp::NamedMember>);

impl NamedMember {
    pub fn from(group: Group, info: GroupMemberInfo) -> Self {
        let inner = imp::NamedMember { group, info };

        Self(inner.into())
    }

    pub fn id(&self) -> i64 {
        self.0.info.uin
    }

    pub fn nickname(&self) -> &str {
        &self.0.info.nickname
    }

    pub fn card_name(&self) -> &str {
        &self.0.info.card_name
    }

    pub fn group(&self) -> &Group {
        &self.0.group
    }
}

mod imp {
    use crate::contact::group::Group;
    use crate::GroupMemberInfo;

    pub struct NamedMember {
        pub group: Group,
        pub info: GroupMemberInfo,
    }

    pub struct AnonymousMember;
}
