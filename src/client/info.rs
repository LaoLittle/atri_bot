use std::sync::atomic::AtomicU8;
use std::sync::RwLock;

pub struct BotAccountInfo {
    pub nickname: RwLock<String>,
    pub age: AtomicU8,
    pub gender: AtomicU8,
}

impl From<ricq::structs::AccountInfo> for BotAccountInfo {
    fn from(
        ricq::structs::AccountInfo {
            nickname,
            age,
            gender,
        }: ricq::structs::AccountInfo,
    ) -> Self {
        Self {
            nickname: nickname.into(),
            age: age.into(),
            gender: gender.into(),
        }
    }
}
