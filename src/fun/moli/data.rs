use ricq::msg::elem::RQElem;
use ricq::structs::GroupMessage;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct MoliMessage {
    pub content: String,
    #[serde(rename = "type")]
    pub chat_type: u8,
    pub from: i64,
    #[serde(rename = "fromName")]
    pub from_name: String,
    pub to: i64,
    #[serde(rename = "toName")]
    pub to_name: String,
}

impl MoliMessage {
    pub fn from_group_message(msg: GroupMessage, sender_name: String) -> Self {
        let mut content = String::new();
        for elem in msg.elements {
            match elem {
                RQElem::Text(s) => content.push_str(&s.content),
                RQElem::At(at) => content.push_str(&at.display),
                RQElem::GroupImage(_) => content.push_str("[图片]"),
                _ => {}
            }
        }

        Self {
            content,
            chat_type: 2,
            from: msg.from_uin,
            from_name: sender_name,
            to: msg.group_code,
            to_name: msg.group_name,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MoliData {
    pub content: String,
    pub typed: u8,
    pub remark: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MoliResponse {
    pub code: String,
    pub message: String,
    pub plugin: Option<String>,
    pub data: Vec<MoliData>,
}
