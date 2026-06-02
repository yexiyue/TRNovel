## 1. 依赖

- [x] 1.1 在 `crates/parse-book-source/Cargo.toml` 添加纯 Rust XPath 依赖(`sxd-document`、`sxd-xpath`)
- [x] 1.2 确认无新增 C 依赖(`cargo tree` 检查不引入 libxml/native 链接)

## 2. 宽松 HTML → XPath 桥(核心)

- [x] 2.1 实现 `html_to_xpath_doc(content) -> Option<...>`:复用 `dom_query`/html5ever 宽松解析,产出可被 `sxd` 求值的文档(序列化往返或遍历建树,择稳)
- [x] 2.2 桥接失败时返回 `None`(由调用方降级为空结果,不 panic)
- [x] 2.3 单元测试:脏 HTML(未闭合标签、无引号属性、void 元素)能被桥接并求值

## 3. `xpath_extract`(值规则)

- [x] 3.1 实现 XPath 求值 → `Value` 四分支映射(String/Number/Boolean 直取;Nodeset 按 `index` 取节点)
- [x] 3.2 元素节点套用 `Extract`(text/ownText/html/innerHtml/outerHtml/attr),复用 `clean_html`
- [x] 3.3 属性/文本节点取其字符串值(trim)
- [x] 3.4 空匹配 → 空串;XPath 表达式语法非法 → `EvalError::Xpath`(或复用既有错误变体)
- [x] 3.5 接入 `extract()` 分发:替换 backend.rs:29 的 `Err(Unsupported)`

## 4. `xpath_select_all`(列表规则)

- [x] 4.1 实现元素节点集 → 各 outerHTML 子上下文(对齐 CSS `select_all`)
- [x] 4.2 属性/文本节点集 → 各字符串值;空集 → 空 Vec
- [x] 4.3 接入 `select_all()` 分发:替换 backend.rs:61 的 `Err(Unsupported)`

## 5. 测试与验收

- [x] 5.1 单测:`//div[@class='x']` 元素 + `extract:text`
- [x] 5.2 单测:`//a/@href` 标量属性(忽略 extract)
- [x] 5.3 单测:`//p/text()` 标量文本
- [x] 5.4 单测:`select_all` 列表 → item 规则在子上下文继续求值(可混用 xpath/css)
- [x] 5.5 单测:空匹配降级为空、非法表达式报错
- [x] 5.6 `cargo test -p parse-book-source` 全绿
- [x] 5.7 `cargo clippy --all-targets --all-features --workspace -- -D warnings` 与 `cargo fmt --all --check`

## 6. 文档

- [ ] 6.1 在 `docs/` 书源参考的规则页补充 `via: "xpath"` 用法与"良构度要求高于 CSS、脏站优先 CSS"的提示
