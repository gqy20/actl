//! window.rs 单测(file-lines 拆分,2026-10-12)。
use super::{crash_signature, is_shell_surface};
#[cfg(test)]
mod crash_signature_tests {
    use super::crash_signature;

    /// WER 崩溃框族签名:两起真机事故形态+本地化变体全命中。
    #[test]
    fn wcrash_family_titles_match() {
        assert_eq!(
            crash_signature("WMExternal.exe - Fail Fast Exception"),
            Some("fail_fast"),
            "2026-10-12 两起真机事故的原始标题"
        );
        assert_eq!(crash_signature("Foo.exe - 应用程序错误"), Some("app_error"));
        assert_eq!(crash_signature("Bar 已停止工作"), Some("stopped_working"));
        assert_eq!(
            crash_signature("Bar 已停止响应"),
            Some("stopped_responding")
        );
        assert_eq!(crash_signature("Windows 错误报告"), Some("wer"));
    }

    /// 非崩溃窗口不命中(签名必须应用无关且宁缺勿滥——正常窗口误报会
    /// 把 residue-report 变噪音):业务确认框/保存框/普通标题全过。
    #[test]
    fn normal_titles_do_not_match() {
        for clean in [
            "腾讯会议",
            "另存为",
            "确认另存为",
            "WMExternal", // 签名要求标题含崩溃短语,仅进程名不算
            "失败",       // 单字"失败"不是 WER 标题形态
            "无标题 - Notepad",
        ] {
            assert_eq!(crash_signature(clean), None, "{clean:?} must not match");
        }
    }
}

#[cfg(test)]
mod shell_surface_tests {
    use super::is_shell_surface;

    #[test]
    fn guard_ignores_non_explorer_programs() {
        // 非 explorer 进程:类名判定短路前即 false(零 hwnd 传入也安全)
        assert!(!is_shell_surface(
            "C:/Windows/notepad.exe",
            Default::default()
        ));
        assert!(!is_shell_surface("", Default::default()));
    }
}
