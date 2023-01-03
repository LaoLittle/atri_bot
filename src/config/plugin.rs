use serde::{Deserialize, Serialize};

/// 对插件产生错误的态度
#[derive(Serialize, Deserialize, Default)]
pub enum FaultAttitude {
    #[default]
    /// 立即结束程序, 记录堆栈
    FastFault,
    /// 忽略错误, 关闭产生错误的监听器, 记录堆栈
    ///
    /// 可能导致内存泄露或其他问题
    Ignore,
}
