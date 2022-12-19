use crate::client::Client;
use crate::contact::friend::Friend;
use crate::contact::group::Group;
use crate::service::command::{CommandError, CommandResult};
use std::str::FromStr;

pub trait CommandArg: Sized {
    fn from_str(arg: &str) -> CommandResult<Self>;
}

impl<T: FromStr> CommandArg for T {
    fn from_str(arg: &str) -> CommandResult<Self> {
        <T as FromStr>::from_str(arg).map_err(|_| CommandError::IllegalArgument)
    }
}

impl CommandArg for Client {
    fn from_str(arg: &str) -> CommandResult<Self> {
        let id = i64::from_str_radix(arg, 10)?;

        Client::find(id).ok_or_else(|| CommandError::execute_error(format!("无法找到客户端: {id}")))
    }
}

impl CommandArg for Friend {
    fn from_str(arg: &str) -> CommandResult<Self> {
        let id = i64::from_str_radix(arg, 10)?;

        if let [client] = &Client::list()[..] {
            return client
                .find_friend(id)
                .ok_or_else(|| CommandError::execute_error(format!("无法找到好友: {id}")));
        }

        Err(CommandError::execute_error(
            "无法从多个客户端中找到好友, 请指定客户端, 例 Client:Friend",
        ))
    }
}

impl CommandArg for Group {
    fn from_str(arg: &str) -> CommandResult<Self> {
        let id = i64::from_str_radix(arg, 10)?;

        if let [client] = &Client::list()[..] {
            return client
                .find_group(id)
                .ok_or_else(|| CommandError::execute_error(format!("无法找到群: {id}")));
        }

        Err(CommandError::execute_error(
            "无法从多个客户端中找到群, 请指定客户端, 例 Client:Group",
        ))
    }
}
