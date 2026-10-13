//! actl-browser —— CDP 通道(设计 docs/72):actl 自有浏览器会话的最小同步客户端
//! + 深水层(chromiumoxide 同步门面,bridge)。
//!
//! 范围铁律:
//! - 浅层手写客户端只实现白名单域(Target/Runtime/Page/Browser)——cookie 读取域
//!   (Network/Storage)构造性不可达,命令面永不提供 cookie 导出动词(docs/72 §8);
//!   深水层类型面全域可达但命令面同守此誓——负面 golden 钉死无 cookie 动词;
//! - async 仅存在于 bridge 模块的受控 runtime 之后(ADR-001 修订),crate 对外
//!   API 保持同步;无 unsafe(豁免仅属 actl-uia)、不附着用户已有会话;
//! - 会话文件落 actl 数据目录,launcher 保证 user-data-dir 绝对化与调试端口出现,
//!   否则启动即失败(docs/72 §11 不变量:自启实例必须总能经 CDP 优雅关闭)。

pub mod cdp;
pub mod collect;
pub mod deep;
pub mod ops;
mod scripts;
pub mod session;
pub mod verify;

mod bridge;

pub use verify::VerifyMode;

/// 定位方式(CSS 或 XPath;唯一匹配为硬条件)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorKind {
    Css,
    Xpath,
}

impl SelectorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Css => "css",
            Self::Xpath => "xpath",
        }
    }
}
