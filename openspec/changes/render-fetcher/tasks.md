## 1. browser.rs:渲染 + CDP 拦截取页转正

- [x] 1.1 把 PoC 的 `render_and_probe` 清理为正式「渲染后 DOM」取页:`render_dom(url, ready_for, timeout, headless) -> Result<String>`(返回 outerHTML;就绪轮询 + 超时降级),移除 PoC 的 probe 返回。
- [x] 1.2 把 PoC 的 `render_intercept` 清理为正式「CDP 拦截响应」取页:`render_intercept(url, api_contains, timeout, headless) -> Result<String>`(先 `Network.enable`+挂监听再 `goto`,`getResponseBody` 轮询取,处理 base64 标志,超时降级)。
- [x] 1.3 启动配置抽出公共构造:headless 时省 `with_head()`,其余 args 同 solve/login;复用登录 profile;清 SingletonLock。
- [x] 1.4 错误语义统一为 `FetchError`(就绪超时 / 未拦截 / 浏览器不可用),供上层降级判定。

## 2. schema:per-op 渲染开关

- [x] 2.1 `source.rs` 的 `Request` 加字段:`render: bool`(默认 false)、`readyFor: Option<String>`、`interceptApi: Option<String>`;`deny_unknown_fields` 不破坏。
- [x] 2.2 更新 `references/book-source.schema.json`(gen_schema 重导)+ skill 文档说明 SPA 渲染取页用法。
- [x] 2.3 校验:现有书源 JSON(无新字段)逐字节解析等价(加测试)。

## 3. engine:取页路由

- [x] 3.1 engine 取页处:若当前 op 的 `request.render` 为真 ∧ 浏览器可用 ∧ 已授权 → 走渲染 fetcher(`interceptApi` 选拦截、否则 `readyFor` 选 DOM),默认 headless;否则 reqwest(现状)。
- [x] 3.2 拦截/渲染结果接现有 `list`/`item` 规则:JSON 走 `via:"json"`,DOM 走 CSS;`book_id` → `bookUrl` 经 template 拼接。
- [x] 3.3 无浏览器/降级路径:渲染 op 失败不冒泡、不影响其它 op;`diagnose` 对渲染搜索给精确诊断。

## 4. 番茄搜索接通(验证）

- [x] 4.1 实测搜索字体 `c207f68a84deae3.woff2` 跨关键词/会话是否稳定(跑两个词比对);据此定「单独 fontMap」或「读 DOM 已解码文本」。
- [x] 4.2 `gen-fontmap` 生成搜索字体表(若走单独表);`fanqie-web.v2.json` 加 `search`:`request.render=true`+`interceptApi:"search_book/v1"`,`list/item` 用 `via:json`(`book_id`→bookUrl),name/author 用搜索 fontMap。
- [x] 4.3 `trn doctor fanqie-web.v2.json`:搜索项转 ✓;`trn import` 重新导入。

## 5. 收尾

- [x] 5.1 `cargo clippy --all-targets --all-features --workspace -- -D warnings` + `cargo fmt` + `cargo test -p parse-book-source` 通过。
- [x] 5.2 保留 `examples/spa_search_poc.rs` 作手动验收;在 reverse-engineering.md 把「整页 JS 渲染→暂不支持」改为「渲染型 fetcher 可做」。
- [x] 5.3 更新 CHANGELOG / 相关 OpenSpec 交叉引用(browser-fetcher)。
