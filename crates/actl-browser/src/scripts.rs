//! 页面侧 JS 脚本构造器(纯字符串构造,无 IO)——从 ops.rs 按内聚拆出
//! (file-lines 治理)。选择器一律以 JSON 字符串字面量嵌入(serde_json
//! 转义,防注入);脚本约定返回 JSON.stringify 结果,由 ops::eval_json
//! 经 Runtime.evaluate 执行并解析。

use crate::SelectorKind;

/// 白名单属性 → 固定 JS 表达式(非用户输入,无注入面)
pub(crate) fn property_expr(property: &str) -> &'static str {
    match property {
        "value" => "(e.value===undefined?null:String(e.value))",
        "href" => "(e.href?String(e.href):null)",
        "name" => "(e.getAttribute('name'))",
        "id" => "(e.id||null)",
        "checked" => "(e.checked===undefined?null:(e.checked?'true':'false'))",
        "tag" => "(e.tagName||null)",
        _ => "(e.textContent==null?'':String(e.textContent))",
    }
}

/// JS 定位脚本:唯一匹配为硬条件,返回 JSON.stringify 的结果对象。
/// 选择器以 JSON 字符串字面量嵌入(serde_json 转义,防注入);
/// property 附带白名单属性值(value/href 等,textContent 之外的读取通道)。
pub(crate) fn locate_script(kind: SelectorKind, value: &str, property: Option<&str>) -> String {
    let embedded = serde_json::to_string(value).unwrap_or_default();
    let prop_field = match property {
        Some(p) => format!(",property:{}", property_expr(p)),
        None => String::new(),
    };
    match kind {
        SelectorKind::Css => format!(
            "(function(){{var els=document.querySelectorAll({embedded});\
             if(els.length===0)return JSON.stringify({{matches:0}});\
             if(els.length>1)return JSON.stringify({{matches:els.length}});\
             var e=els[0];\
             return JSON.stringify({{matches:1,text:e.textContent==null?'':String(e.textContent),\
             tag:e.tagName,id:e.id||null,name:e.getAttribute('name')||null{prop_field}}});}})()"
        ),
        SelectorKind::Xpath => format!(
            "(function(){{var it=document.evaluate({embedded},document,null,\
             XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,null);\
             if(it.snapshotLength===0)return JSON.stringify({{matches:0}});\
             if(it.snapshotLength>1)return JSON.stringify({{matches:it.snapshotLength}});\
             var e=it.snapshotItem(0);\
             return JSON.stringify({{matches:1,text:e.textContent==null?'':String(e.textContent),\
             tag:e.tagName||null{prop_field}}});}})()"
        ),
    }
}

/// 枚举脚本(--all,docs/73 round7 痛点收口):返回**全部**匹配,不在 JS
/// 侧判唯一。结构 {total, elements:[…]}——elements 每项带 text/property/
/// tag/id,截断与去重不在 JS 做(诚实枚举,Rust 侧按 limit 截断上报);
/// 零匹配 total:0(枚举语义,空集合法——不是 NOT_FOUND)。
pub(crate) fn locate_all_script(kind: SelectorKind, value: &str, property: Option<&str>) -> String {
    let embedded = serde_json::to_string(value).unwrap_or_default();
    let prop_field = match property {
        Some(p) => format!(",property:{}", property_expr(p)),
        None => String::new(),
    };
    match kind {
        SelectorKind::Css => format!(
            "(function(){{var els=document.querySelectorAll({embedded});\
             var out=[];for(var i=0;i<els.length;i++){{var e=els[i];\
             out.push({{text:e.textContent==null?'':String(e.textContent),\
             tag:e.tagName,id:e.id||null,name:e.getAttribute('name')||null{prop_field}}});}}\
             return JSON.stringify({{matches:els.length,elements:out}});}})()"
        ),
        SelectorKind::Xpath => format!(
            "(function(){{var it=document.evaluate({embedded},document,null,\
             XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,null);\
             var out=[];for(var i=0;i<it.snapshotLength;i++){{var e=it.snapshotItem(i);\
             out.push({{text:e.textContent==null?'':String(e.textContent),\
             tag:e.tagName||null,name:e.getAttribute('name')||null{prop_field}}});}}\
             return JSON.stringify({{matches:it.snapshotLength,elements:out}});}})()"
        ),
    }
}

/// 深度穿透脚本(--deep,2026-10-11 spec):**开放 shadow DOM 递归遍历**——
/// 在 document 与每个 `element.shadowRoot`(open 模式;closed 对页面 JS
/// 结构性不可见,如实边界)内执行选择器,收集全部命中。
/// 预算有界:访问节点 ≤50000、收集元素 ≤5000,超限停止并把
/// `traversal_capped` 如实上报(matches 变成下界,不谎称全量)。
/// 返回枚举形态 {matches, elements, traversal_capped}——唯一模式由 Rust
/// 侧在 elements[0] 上判定,与 --all 共用同一脚本。
pub(crate) fn locate_deep_script(
    kind: SelectorKind,
    value: &str,
    property: Option<&str>,
) -> String {
    let embedded = serde_json::to_string(value).unwrap_or_default();
    let prop_field = match property {
        Some(p) => format!(",property:{}", property_expr(p)),
        None => String::new(),
    };
    let query = match kind {
        SelectorKind::Css => format!(
            "try{{var els=root.querySelectorAll({embedded});\
             for(var i=0;i<els.length&&out.length<5000;i++)out.push(els[i]);}}catch(e){{}}"
        ),
        SelectorKind::Xpath => format!(
            "try{{var it=document.evaluate({embedded},root,null,\
             XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,null);\
             for(var i=0;i<it.snapshotLength&&out.length<5000;i++)out.push(it.snapshotItem(i));}}catch(e){{}}"
        ),
    };
    format!(
        "(function(){{\
         var out=[];var visited=0;var capped=false;\
         function collect(root){{{query}\
         try{{var all=root.querySelectorAll('*');\
         for(var j=0;j<all.length;j++){{\
         if(visited>50000){{capped=true;return;}}\
         visited++;\
         var sr=all[j].shadowRoot;\
         if(sr)collect(sr);}}}}catch(e){{}}\
         }}\
         collect(document);\
         function deepText(e){{\
         var s=(e.textContent==null?'':String(e.textContent));\
         if(e.shadowRoot){{\
         var w=document.createTreeWalker(e.shadowRoot,NodeFilter.SHOW_TEXT,{{\
         acceptNode:function(n){{var p=n.parentElement;\
         if(p&&(p.tagName==='STYLE'||p.tagName==='SCRIPT'||p.tagName==='TEMPLATE'))\
         return NodeFilter.FILTER_REJECT;return NodeFilter.FILTER_ACCEPT;}}}},false);\
         var node;while((node=w.nextNode()))s+=node.nodeValue;\
         var all=e.shadowRoot.querySelectorAll('*');\
         for(var i=0;i<all.length;i++){{if(all[i].shadowRoot)s+=deepText(all[i]);}}}}\
         return s;}}\
         var payload=[];\
         for(var k=0;k<out.length;k++){{var e=out[k];\
         payload.push({{text:deepText(e),\
         tag:e.tagName,id:e.id||null,name:e.getAttribute('name')||null{prop_field}}});}}\
         return JSON.stringify({{matches:out.length,elements:payload,\
         traversal_capped:capped}});}})()"
    )
}

/// 弹层扫描脚本(--no-popups,2026-10-11 v1):只计**可见**命中(零布局盒
/// =display:none 族,不计);样本有界(≤5 条,每条 text ≤200 字符折叠空白)。
/// 选择器语法错误走 error 字段如实上报(PROTOCOL),不静默当零弹层。
pub(crate) fn popup_scan_script(selector: &str) -> String {
    let embedded = serde_json::to_string(selector).unwrap_or_default();
    format!(
        "(function(){{\
         try{{\
         var els=document.querySelectorAll({embedded});\
         var total=0;var samples=[];\
         for(var i=0;i<els.length;i++){{var e=els[i];\
         if(e.getClientRects().length===0)continue;\
         total++;\
         if(samples.length<5){{\
         var t=e.textContent==null?'':String(e.textContent);\
         samples.push({{tag:e.tagName,id:e.id||null,\
         text:t.replace(/\\s+/g,' ').trim().slice(0,200)}});}}}}\
         return JSON.stringify({{total:total,samples:samples}});\
         }}catch(err){{return JSON.stringify({{error:String(err)}});}}\
         }})()"
    )
}

/// 页级滚动脚本(browser-collect,2026-10-12 spec):滚动一屏(约 0.9 视口高,
/// 与 UIA collect 的页级滚动同语义)并返回滚动事实。**at_bottom 是"继续滚
/// 的门"而非完成判据**:没到底时不允许空连收敛(懒加载分段可能在更下方,
/// B 站视频页评论区分页实证——播放器占 2-3 屏,纯空连两轮会在抵达评论
/// 区前提前收敛);到了底完成权威仍是空连两轮差分(scrollHeight 会随
/// 加载增长,触底后新内容出现会再滚——UIA collect 位置信号否证同款教训)。
pub(crate) fn scroll_page_script() -> String {
    "(function(){\
     var h=Math.max(document.documentElement.clientHeight,window.innerHeight||0);\
     window.scrollBy(0,Math.floor(h*0.9));\
     var d=document.documentElement;\
     var at_bottom=(window.scrollY||0)+h>=(d.scrollHeight||0)-2;\
     return JSON.stringify({\
     scrollY:Math.round(window.scrollY||0),\
     innerHeight:Math.round(h),\
     scrollHeight:Math.round(d.scrollHeight||0),\
     at_bottom:at_bottom});})()"
        .into()
}

/// 元素包围盒脚本(browser-screenshot/challenge-solve 的坐标面):唯一
/// 匹配的 getBoundingClientRect(CSS px,视口系)+ 视口尺寸。0/多命中
/// 如实上报(唯一语义与 read/click 同判)。
pub(crate) fn element_box_script(kind: SelectorKind, value: &str) -> String {
    let embedded = serde_json::to_string(value).unwrap_or_default();
    let locate = match kind {
        SelectorKind::Css => format!("document.querySelectorAll({embedded})"),
        SelectorKind::Xpath => format!(
            "document.evaluate({embedded},document,null,\
             XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,null)"
        ),
    };
    format!(
        "(function(){{\
         var root={locate};\
         var n=(root&&root.length!==undefined)?root.length:\
         (root&&root.snapshotLength!==undefined?root.snapshotLength:0);\
         if(n===0)return JSON.stringify({{matches:0}});\
         if(n>1)return JSON.stringify({{matches:n}});\
         var e=(root.length!==undefined)?root[0]:root.snapshotItem(0);\
         var r=e.getBoundingClientRect();\
         return JSON.stringify({{matches:1,\
         x:Math.round(r.x*100)/100,y:Math.round(r.y*100)/100,\
         width:Math.round(r.width*100)/100,height:Math.round(r.height*100)/100,\
         vw:window.innerWidth,vh:window.innerHeight}});}})()"
    )
}

/// 挑战信号采集脚本(browser-verify --challenge / challenge-solve):
/// title + script/iframe src 清单(有界) + 预置 DOM 标记命中。标记选择器
/// 全集与 actl_core::challenge::MARKER_IDS 两侧同步演进(表侧单测钉住)。
/// `window._cf_chl_opt` 是脚本环境标记,经 in 检查取布尔。
pub(crate) fn challenge_scan_script() -> String {
    let markers: Vec<String> = actl_core::challenge::MARKER_IDS
        .iter()
        .map(|(id, selector)| {
            let probe = if selector.starts_with("window.") {
                format!("({selector}!==undefined)")
            } else {
                // 选择器以 JSON 字面量嵌入(serde 转义——表内含单引号属性
                // 选择器,裸单引号包裹会撞 JS 语法)
                format!(
                    "!!document.querySelector({})",
                    serde_json::to_string(selector).unwrap_or_default()
                )
            };
            format!("'{id}':{probe}")
        })
        .collect();
    format!(
        "(function(){{\
         var scripts=[];var ss=document.querySelectorAll('script[src]');\
         for(var i=0;i<ss.length&&scripts.length<120;i++)\
         scripts.push(String(ss[i].src).slice(0,512));\
         var iframes=[];var fs=document.querySelectorAll('iframe[src]');\
         for(var j=0;j<fs.length&&iframes.length<60;j++)\
         iframes.push(String(fs[j].src).slice(0,512));\
         var markers={{{}}};\
         return JSON.stringify({{title:document.title||'',\
         scripts:scripts,iframes:iframes,markers:markers}});}})()",
        markers.join(",")
    )
}
