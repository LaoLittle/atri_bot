use crate::contact::friend::Friend;
use crate::contact::group::Group;
use crate::contact::member::Member;

pub mod friend;
pub mod group;
pub mod member;

#[derive(Clone)]
pub enum Contact {
    Friend(Friend),
    Group(Group),
    Member(Member),
    Stranger,
}

pub trait HasSubject {
    fn subject(&self) -> Contact;
}
