//! 人机验证挑战检测的签名表(纯逻辑,单测主场):「呈现在面前的挑战」的
//! 判别面——页面事实(title/script src/iframe src/DOM 标记)对已知厂商
//! 形态的分型。检测只识别不绕过;签名表来自公开文档与官方部署形态
//! (docs 调研轮:GeeTest/腾讯防水墙/阿里云/顶象/易盾/Cloudflare/
//! reCAPTCHA/hCaptcha/Arkose/DataDome 的 script/iframe 域名与容器特征)。
//!
//! 诚实边界:厂商 class/id 不承诺跨版本稳定(官方文档明示),签名命中=
//! 「形似已知挑战」的判读建议,未命中≠无挑战(自绘挑战不在表内);
//! 命中家族供解题循环选路线,不构成对站点的任何断言。

/// 页面侧采集事实(DOM 采集脚本产出,Rust 侧匹配)。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChallengePage {
    pub title: String,
    /// script[src] 清单(≤120 条,每条 ≤512 字节,采集侧有界)。
    pub scripts: Vec<String>,
    /// iframe[src] 清单(≤60 条)。
    pub iframes: Vec<String>,
    /// DOM 标记命中集(采集脚本预置选择器 → 布尔)。
    pub markers: Vec<String>,
}

/// 一条挑战判读:家族 + 命中证据(信号明细,审计与路线选择共用)。
#[derive(Debug, Clone, PartialEq)]
pub struct ChallengeHit {
    /// 家族 id(snake_case,与调用方路线表对齐)。
    pub family: &'static str,
    /// 人类可读标签。
    pub label: &'static str,
    /// 命中信号明细(如 "script:geetest.com")。
    pub signals: Vec<String>,
}

/// 挑战形态家族:一个家族 = 一组 url 子串信号 + 一组 DOM 标记信号。
/// 任一信号命中即判「形似该家族」;多家族同时命中时全部如实上报
/// (调用方按证据数排序选路线)。
struct Family {
    family: &'static str,
    label: &'static str,
    /// script/iframe src 的子串信号(大小写不敏感)。
    urls: &'static [&'static str],
    /// DOM 标记名(采集脚本 marker id)。
    markers: &'static [&'static str],
    /// title 子串(整页插页形态)。
    titles: &'static [&'static str],
}

/// 采集脚本预置的 DOM 标记全集(marker id → CSS 选择器在采集侧)。
/// Rust 侧只认这里的 id——表与脚本两侧同步演进。
pub const MARKER_IDS: &[(&str, &str)] = &[
    ("cf_form", "#challenge-form"),
    ("cf_chl_opt", "window._cf_chl_opt"),
    ("turnstile", ".cf-turnstile"),
    ("g_recaptcha", ".g-recaptcha,#recaptcha_image"),
    ("h_captcha", ".h-captcha"),
    ("geetest", "[class*='geetest']"),
    ("aliyun_captcha", "#captcha-element,#captcha-button"),
    ("dingxiang", "[data-dx-captcha],#dx-captcha"),
    ("yidun", "[class*='yidun']"),
];

const FAMILIES: &[Family] = &[
    Family {
        family: "cloudflare_interstitial",
        label: "Cloudflare 全页挑战(Just a moment...)",
        urls: &["challenges.cloudflare.com/cdn-cgi/challenge-platform"],
        markers: &["cf_form", "cf_chl_opt"],
        titles: &["just a moment", "attention required"],
    },
    Family {
        family: "cloudflare_turnstile",
        label: "Cloudflare Turnstile 嵌入式组件",
        urls: &["challenges.cloudflare.com/turnstile"],
        markers: &["turnstile"],
        titles: &[],
    },
    Family {
        family: "geetest",
        label: "极验 GeeTest(滑块/点选)",
        urls: &[".geetest.com/", ".geevisit.com/", ".gsensebot.com/"],
        markers: &["geetest"],
        titles: &[],
    },
    Family {
        family: "tencent_captcha",
        label: "腾讯防水墙 TCaptcha",
        urls: &["captcha.gtimg.com", "turing.captcha.qcloud.com"],
        markers: &[],
        titles: &[],
    },
    Family {
        family: "aliyun_captcha",
        label: "阿里云验证码",
        urls: &["aliyuncaptcha", "captcha.aliyun.com"],
        markers: &["aliyun_captcha"],
        titles: &[],
    },
    Family {
        family: "dingxiang",
        label: "顶象验证码",
        urls: &["dingxiangcaptcha", "dingxiang-inc.com"],
        markers: &["dingxiang"],
        titles: &[],
    },
    Family {
        family: "yidun",
        label: "网易易盾行为式验证",
        urls: &["cstaticdun.126.net", "c.dun.163.com"],
        markers: &["yidun"],
        titles: &[],
    },
    Family {
        family: "recaptcha",
        label: "Google reCAPTCHA",
        urls: &[
            "google.com/recaptcha",
            "recaptcha.net/recaptcha",
            "gstatic.com/recaptcha",
        ],
        markers: &["g_recaptcha"],
        titles: &[],
    },
    Family {
        family: "hcaptcha",
        label: "hCaptcha",
        urls: &["hcaptcha.com"],
        markers: &["h_captcha"],
        titles: &[],
    },
    Family {
        family: "arkose",
        label: "Arkose Labs(FunCaptcha)",
        urls: &["arkoselabs.com", "funcaptcha.com"],
        markers: &[],
        titles: &[],
    },
    Family {
        family: "datadome",
        label: "DataDome 挑战",
        urls: &["captcha-delivery.com", "ct.captcha-delivery.com"],
        markers: &[],
        titles: &[],
    },
];

/// 逐家族匹配:src(script+iframe)子串、DOM 标记、title 子串任一命中
/// 即收进结果;证据数排序(降序),同证据数按家族字典序(确定性)。
pub fn detect(page: &ChallengePage) -> Vec<ChallengeHit> {
    let lower_title = page.title.to_lowercase();
    let mut hits: Vec<ChallengeHit> = Vec::new();
    for family in FAMILIES {
        let mut signals: Vec<String> = Vec::new();
        let urls: Vec<&String> = page
            .scripts
            .iter()
            .chain(page.iframes.iter())
            .filter(|src| {
                let lower = src.to_lowercase();
                family.urls.iter().any(|u| lower.contains(u))
            })
            .collect();
        for src in urls {
            signals.push(format!("url:{src}"));
        }
        for marker in family.markers {
            if page.markers.iter().any(|m| m == marker) {
                signals.push(format!("marker:{marker}"));
            }
        }
        for title in family.titles {
            if lower_title.contains(title) {
                signals.push(format!("title:{title}"));
            }
        }
        if !signals.is_empty() {
            hits.push(ChallengeHit {
                family: family.family,
                label: family.label,
                signals,
            });
        }
    }
    hits.sort_by(|a, b| {
        b.signals
            .len()
            .cmp(&a.signals.len())
            .then_with(|| a.family.cmp(b.family))
    });
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(title: &str, scripts: &[&str], iframes: &[&str], markers: &[&str]) -> ChallengePage {
        ChallengePage {
            title: title.into(),
            scripts: scripts.iter().map(|s| s.to_string()).collect(),
            iframes: iframes.iter().map(|s| s.to_string()).collect(),
            markers: markers.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn cloudflare_interstitial_by_title_and_marker() {
        let hits = detect(&page("Just a moment...", &[], &[], &["cf_form"]));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].family, "cloudflare_interstitial");
        assert!(hits[0].signals.contains(&"title:just a moment".to_string()));
        assert!(hits[0].signals.contains(&"marker:cf_form".to_string()));
    }

    #[test]
    fn geetest_by_script_domain() {
        let hits = detect(&page(
            "登录",
            &["https://static.geetest.com/static/js/gt.0.5.0.js"],
            &[],
            &[],
        ));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].family, "geetest");
        assert!(hits[0].signals[0].contains("static.geetest.com"));
    }

    #[test]
    fn tencent_iframe_and_multi_family_ranks_by_evidence() {
        let hits = detect(&page(
            "验证",
            &["https://turing.captcha.qcloud.com/TJCaptcha.js"],
            &["https://turing.captcha.gtimg.com/cap_union_new_show"],
            &["geetest"],
        ));
        // 腾讯两个 url 信号 vs 极验一个 marker 信号——证据数排序
        let families: Vec<&str> = hits.iter().map(|h| h.family).collect();
        assert_eq!(families, vec!["tencent_captcha", "geetest"]);
    }

    #[test]
    fn clean_page_has_no_hits() {
        assert!(detect(&page("百度一下", &["https://example.com/app.js"], &[], &[])).is_empty());
    }

    #[test]
    fn marker_ids_cover_all_table_markers() {
        // 表内 marker 必须在采集脚本预置全集里——两侧漂移在这里拦下
        for family in FAMILIES {
            for marker in family.markers {
                assert!(
                    MARKER_IDS.iter().any(|(id, _)| id == marker),
                    "family {} marker {} missing from MARKER_IDS",
                    family.family,
                    marker
                );
            }
        }
    }
}
