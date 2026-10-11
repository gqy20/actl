//! actl-browser —— CDP 通道(设计 docs/72):actl 自有浏览器会话的最小同步客户端。
//!
//! 范围铁律:
//! - 只实现白名单域(Target/Runtime/Page/Browser)——cookie 读取域(Network/Storage)
//!   构造性不可达,命令面永不提供 cookie 导出动词(docs/72 §8);
//! - 无 async/tokio(ADR-001)、无 unsafe(豁免仅属 actl-uia)、不附着用户已有会话;
//! - 会话文件落 actl 数据目录,launcher 保证 user-data-dir 绝对化与调试端口出现,
//!   否则启动即失败(docs/72 §11 不变量:自启实例必须总能经 CDP 优雅关闭)。

pub mod cdp;
pub mod collect;
pub mod ops;
mod scripts;
pub mod session;

pub use ops::VerifyMode;

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
