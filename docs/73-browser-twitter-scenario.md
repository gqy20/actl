# 73 · 浏览器 CDP 通道场景实测:x.com(Twitter)未登录全链

日期:2026-10-02。性质:场景实测轨(不进 CI;过程证据留 `target/browser-twitter-20261002/`,
逐轮 jsonl)。用户指令"直接拿真机测试,试一下 Twitter"。环境:开发机 Win11 zh-cn,
系统 msedge Edg/154.0.4258.48,本机 clash(WinHTTP 代理 127.0.0.1:7890 + TUN fake-ip),
actl v1 `browser-*` 命令面(本日 spec,ff355ba + 当场增补)。
关联:[72 设计与实现](72-browser-cdp-integration.md) · [06 §3 浏览器会话](06-design-blueprint.md) · [16 UIA 赛道浏览器场景](16-browser-scenarios.md)

## 结论先行

v1 命令面在真实敌意站点上**通道层全通**:导航(经代理)、内容读取(title/meta/锚点
href 枚举)、唯一匹配纪律(含漂移拒绝)、位置谓词消歧、真实 DOM 守卫写(间歇页"继续"
按钮与登录锚点各一次)、每轮优雅关闭。**未登录能走到的终点=onboarding 登录流首屏**
(用户名输入框在场);凭据操作不做(一次性人工登录归持久 profile 协作暂停点,docs/72 §4)。

## 三道真实壁垒与对策(全部真机实证)

| 壁垒 | 实证 | 对策(已落地/待办) |
|---|---|---|
| headless UA 拒服务 | curl A/B:同代理下 Headless UA→**403**,普通 UA→200 | `--visible`(UA 不带 Headless 标记)当场可用;headless 模式加 UA 覆盖选项→待办 |
| 浏览器 DoH 绕过 TUN fake-ip | OS 层 curl 直连/代理均 200,headless Edge 却 chrome-error(DNS 真实 IP 直连被重置) | **spec 增补 `--proxy HOST:PORT`**(仅裸 host:port,防注入面);当场落地并验证 |
| 导航网络失败静默 | 增补前 chrome-error 错误页被当成功(ok:true 只在 url 里露出) | **spec 增补:Page.navigate errorText 显式失败(INTERNAL+evidence)**;落地后同场景立即正确报 `net::ERR_HTTP_RESPONSE_CODE_FAILURE` |

## 逐轮要点(8 轮,round1-8 jsonl 为准)

1. round1(无代理 headless):navigate 得 chrome-error——挖出上述第 2、3 两个缺口;
2. round2(--proxy headless):errorText 显式上报 `ERR_HTTP_RESPONSE_CODE_FAILURE`——
   即 UA 壁垒(403)的网络层表现;
3. round3(--proxy --visible):**x.com 真实加载**(title"X。尽是新鲜事 / X"、中文 meta
   描述全文读出)——三件套成立;
4. round4(发现式探测):SPA 已水化(21 锚点)但 zh 未登录首页是**间歇页**:标准
   loginButton testid、/login href 全部不在场;唯一按钮文本"继续"(UTF-8 输出侧读出,
   参数侧全 ASCII,守 bash 中文污染纪律);
5. round5:按钮数跨加载漂移(1→3),**AMBIGUOUS 护栏正确拒绝盲写**——守卫语义在真实
   站点漂移下的活体证明;
6. round6:位置谓词 `(//button)[k]` 消歧 + UTF-8 字节匹配选定"继续",**守卫写成功**
   (clicked:true,text_after"继续");
7. round7:`(//a/@href)[k]` **属性节点枚举**破解导航结构——登录路由实为
   `/i/jf/onboarding/web?mode=login&redirect_after_login=%2F`(页脚 tos/privacy/grok/
   下载 App 全部可枚举);contains(@href,"login") 4 处匹配;
8. round8:首个 mode=login 锚点守卫写成功,**URL 推进到 onboarding 登录流**
   (`.../i/jf/onboarding/web?mode=login...`),页面 2 个 autocomplete 输入框在场
   (`input[autocomplete]` AMBIGUOUS=2);`input[name="text"]` 猜测名不符→NOT_FOUND,
   如实记录。

## 挖出的 v1 缺口(进 docs/72 §13 开放项)

- **read 只读 textContent**:input 值读不到(空文本),登录流字段识别受限→需要
  `--property`(value/href/name 等)读取通道;
- **无输入写动词**(`browser-type`):登录/搜索类流程的天然后续;
- ~~headless 模式 UA 覆盖(`--user-agent` 级选项):不牺牲无头形态过 UA 壁垒。~~
  (2026-10-12 收口:`browser-launch --user-agent`(可打印 ASCII,1-512 字节)落地——
  只覆盖 UA 字符串不伪装指纹面,headless 真机对照实证 `navigator.userAgent` 精确
  等于覆盖值;[06 浏览器节](06-design-blueprint.md))
- ~~epub 临时 profile 删除竞争:7 轮中 2 次 `ephemeral_profile_removed:false`
  (浏览器句柄延迟释放)→close 后延迟重试。~~
  (2026-10-12 收口:finish_close 单次吞错→有界退避重试(200/400/800/1600ms,
  上界 ~3s),独占句柄活体竞争单测复现并验证重试路径收口)
- ~~选择器索引消歧助手(UIA 轨 `role:#N` 的等价物):本轮靠位置谓词 xpath 手工达成。~~
  (2026-10-11 更强收口:`browser-read --all [--limit N]` 枚举形态落地——列表语义
  零命中=空集非 NOT_FOUND,位置谓词绕行不再需要;[06 浏览器节](06-design-blueprint.md))

## 纪律记录

- 全程未触碰凭据;未登录态即停,登录动作归用户(持久 profile 一次性人工登录);
- 每轮 browser-close 优雅关闭,无 taskkill;急停状态每轮前置核对(无急停);
- bash 参数侧全 ASCII(中文经 GBK 污染纪律),中文仅出现在读取输出与 UTF-8 字节匹配。

> 后记(同日晚些):本轮结束后 `--visible` 语义翻转为**默认有头**(`--headless` 变为
> 后台选项)——本记录中的 `--visible` 用法对应翻转前的旗标,证据本身不变。
