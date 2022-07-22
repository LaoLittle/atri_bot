use ricq::structs::GroupMemberPermission;

#[derive(Debug, Default, Clone)]
pub struct GroupMemberSharedInfo {
    pub group_code: i64,
    pub uin: i64,
    pub gender: u8,
    pub nickname: String,
    pub card_name: String,
    pub level: u16,
    pub join_time: i64,
    pub special_title: String,
    pub special_title_expire_time: i64,
    pub permission: GroupMemberPermission,
}