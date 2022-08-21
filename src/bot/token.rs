use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, prost::Message)]
pub struct Token {
    #[prost(int64, tag = "1")]
    pub uin: i64,
    #[prost(bytes = "vec", tag = "2")]
    pub d2: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub d2key: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    pub tgt: Vec<u8>,
    #[prost(bytes = "vec", tag = "5")]
    pub srm_token: Vec<u8>,
    #[prost(bytes = "vec", tag = "6")]
    pub t133: Vec<u8>,
    #[prost(bytes = "vec", tag = "7")]
    pub encrypted_a1: Vec<u8>,
    #[prost(bytes = "vec", tag = "8")]
    pub out_packet_session_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "9")]
    pub tgtgt_key: Vec<u8>,
    #[prost(bytes = "vec", tag = "10")]
    pub wt_session_ticket_key: Vec<u8>,
}

impl From<ricq::client::Token> for Token {
    fn from(rq: ricq::client::Token) -> Self {
        let ricq::client::Token {
            uin,
            d2,
            d2key,
            tgt,
            srm_token,
            t133,
            encrypted_a1,
            out_packet_session_id,
            tgtgt_key,
            wt_session_ticket_key,
        } = rq;

        Self {
            uin,
            d2,
            d2key,
            tgt,
            srm_token,
            t133,
            encrypted_a1,
            out_packet_session_id,
            tgtgt_key,
            wt_session_ticket_key,
        }
    }
}

impl From<Token> for ricq::client::Token {
    fn from(token: Token) -> Self {
        let Token {
            uin,
            d2,
            d2key,
            tgt,
            srm_token,
            t133,
            encrypted_a1,
            out_packet_session_id,
            tgtgt_key,
            wt_session_ticket_key,
        } = token;

        Self {
            uin,
            d2,
            d2key,
            tgt,
            srm_token,
            t133,
            encrypted_a1,
            out_packet_session_id,
            tgtgt_key,
            wt_session_ticket_key,
        }
    }
}
