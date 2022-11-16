use crate::contact::friend::Friend;
use crate::contact::group::Group;
use crate::contact::member::Member;

pub mod friend;
pub mod group;
pub mod member;

pub enum Contact {
    Friend(Friend),
    Group(Group),
    Member(Member),
    Stranger,
}

impl Contact {}

pub trait ContactSubject {
    fn subject(&self) -> Contact;
}
