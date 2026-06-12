## Context

当前 explore 模型把「入口」和「取页」混在一起：`Category { title, url }` 既是 UI 选择项，也是 `Engine::explore(category_url, page, page_size)` 的取页输入；`ExploreOp` 自身再携带 `list`、`item`、`render`、`readyFor`、`interceptApi`、`totalPages`、`hasMore` 等列表页字段。这个结构能覆盖静态分类 URL，但不适合番茄这类站点：分类入口来自 `/api/author/book/category_list/v0/?gender=...`，而真正的书籍列表来自浏览器渲染后的 `/api/author/library/book_list/v0/`。

本次 spike 确认了几个事实：

- `/library/...` 静态 HTML 只有空壳和 JS bundle，没有可直接抽取的分类 DOM 链接。
- 前端 bundle 中分类入口来自 `CATEGORY_LIST = /api/author/book/category_list/v0/`，书籍列表来自 `LIBRARY_LIST = /api/author/library/book_list/v0/`。
- `category_list/v0` 可用普通 HTTP 请求返回分类 JSON；`book_list/v0` 直接 HTTP 请求返回 400，仍需现有 render/intercept 让浏览器完成签名。
- 番茄列表 URL 由筛选 token 组成，例如 `/library/audience1-cat2-262/page_{{page}}?sort=hottes`；其中 `hottes` 是站点 bundle 中的真实排序值。

动态入口会成为后续书源主推方向，因此不保留旧 explore 格式兼容。实现时可以做较重重构，让内部模型直接反映「入口生成」与「列表取页」两个阶段。

## Goals / Non-Goals

**Goals:**

- 将 explore 重构为两阶段模型：`entries` 负责生成入口，`page` 负责用入口变量和页码取书。
- 入口是 `title + vars`，而不是 `title + url`；取页 URL 由 `page.request` 统一生成。
- 支持动态入口源从远端数据抽取并平铺成 `Vec<ExploreEntry>`。
- 支持静态入口源与动态入口源组合，组合能力是新架构的一等能力，不是旧格式兼容。
- 将 search/explore 的列表页执行逻辑收敛到共享 `ListPageSpec` 与 `run_list_page`。
- Fanqie explore 使用 `category_list/v0` 动态生成入口，`book_list/v0` 继续走 render/intercept。

**Non-Goals:**

- 不实现多级筛选 UI。动态入口源输出仍是当前 UI 可显示的扁平列表。
- 不展开状态、字数、排序等所有笛卡尔积；第一版 Fanqie 动态入口以分类维度为主，排序默认最热，并保留少量静态入口。
- 不兼容旧外部书源 explore 格式。
- 不改变 search 点击翻页的既有策略，也不复刻 Fanqie 签名。

## Decisions

**D1 - Explore 分成 `entries` 和 `page` 两个阶段。**

新的概念模型：

```text
ExploreOp
├─ entries: EntrySource
└─ page: ListPageSpec
```

入口加载只负责产生可选择的入口：

```text
ExploreEntry
├─ title: String
└─ vars: BTreeMap<String, String>
```

列表取页只看变量：

```text
vars = baseVars + entry.vars + { page, pageSize }
run_list_page(explore.page, vars) -> BookList
```

这避免把 URL 生成逻辑散落到每个入口里，也让动态入口能携带结构化变量，如 `gender`、`audienceToken`、`catGroup`、`categoryId`、`sort`。

**D2 - EntrySource 使用少量正交形态：static、fetch、chain。**

建议 schema 形态：

```json
{
  "entries": {
    "chain": [
      { "static": [ ... ] },
      { "fetch": { ... } }
    ]
  }
}
```

- `static`：声明少量固定入口，如「全部·最热」「全部·最新」。
- `fetch`：请求远端数据，用 `list` 抽数组，用 `item.title` 和 `item.vars` 生成入口。
- `chain`：按顺序合并多个 entry source；动态源失败时是否保留前面已成功的静态入口由加载策略决定。

保留 `chain` 不是为了兼容旧格式，而是因为真实书源常需要「固定入口 + 动态入口」组合。

**D3 - FetchEntrySource 支持 `forEach` 变量循环。**

Fanqie 分类 API 需要按 `gender=0` 和 `gender=1` 请求两次。与其引入隐式循环语法，不如显式支持：

```json
"forEach": [
  { "gender": "0", "audience": "女生", "audienceToken": "audience0" },
  { "gender": "1", "audience": "男生", "audienceToken": "audience1" }
]
```

每次请求时把 `forEach` 的变量注入模板；抽取每个 item 时，规则上下文同时可见当前 JSON item 与外层循环变量。`forEach` 为空时等价于执行一次。

**D4 - 入口变量用现有 Rule 求值，必要时允许 JS。**

`item.vars` 是 `BTreeMap<String, Rule>`。例如 Fanqie 的 `catGroup` 可由 label 映射：

```json
"catGroup": {
  "js": "result.label === '主题' ? '0' : result.label === '角色' ? '1' : '2'"
}
```

普通字段仍用 `via:json`、`template`、`literal`。这复用现有规则引擎，不为入口转换引入第二套表达式系统。

**D5 - `ListPageSpec` 成为 search/explore 共用取页规格。**

建议结构：

```text
ListPageSpec {
  prelude,
  request,
  list,
  item,
  total_pages,
  has_more,
}
```

`SearchOp` 变成：

```text
SearchOp { page: ListPageSpec }
```

`ExploreOp` 变成：

```text
ExploreOp { entries: EntrySource, page: ListPageSpec }
```

引擎收敛成：

```text
run_list_page(spec, vars, page, page_size) -> BookList
```

现有 render/intercept、双源 `totalPages`、`hasMore`、page cache 等行为挂在共享 runner 上，避免 search/explore 各维护一套分支。

**D6 - UI 保持扁平入口列表，但入口加载改为异步。**

`select_books` 初始化时不再同步调用 `engine.explore_categories()`，而是异步调用 `engine.explore_entries().await`。成功后选中第一个入口；失败时显示错误，若 `chain` 中已有静态入口成功则仍可展示这些入口。

`FindBooksProps.current_explore` 持有 `ExploreEntry`，取页时调用：

```text
engine.explore(&entry, page, page_size)
```

内部使用 entry vars 运行 `explore.page`。

**D7 - Fanqie 第一版动态入口控制规模。**

Fanqie `category_list/v0` 返回主分类、主题、角色、情节等大量项。第一版只生成分类维度入口，避免 UI 列表爆炸：

- 静态入口：`全部·最热`、`全部·最新`。
- 动态入口：`女生·<label>·<name>`、`男生·<label>·<name>`，排序默认 `hottes`。

不把状态、字数、排序与分类做笛卡尔积。后续如果需要，可以再用独立 entry source 或 UI 搜索优化处理。

## Risks / Trade-offs

- [破坏旧书源格式] → 本次明确不兼容旧外部格式；仓库内书源、schema、docs、tests 同步迁移。
- [动态入口数量过多，分类弹窗难用] → Fanqie 第一版限制展开维度，不生成状态/字数/排序笛卡尔积。
- [入口加载失败导致无分类] → `chain` 支持静态入口和动态入口组合；实现时保留已成功入口并上报动态源错误。
- [重构影响 search/explore 公共路径] → 增加单元测试覆盖静态入口、动态入口、render/intercept list page、hasMore/totalPages。
- [JS 规则在未启用 `js` feature 时不可用] → Fanqie 动态入口若使用 JS 转换，应确保主程序默认 feature 覆盖；无 JS 时返回明确 unsupported 错误。
- [Fanqie 站点参数变动] → 动态入口从 `category_list/v0` 获取分类项，可减少手写分类漂移；URL token 生成仍需测试锁定。

## Migration Plan

1. 重建 source model 和 schema，删除旧 `ExploreOp.categories`/内联列表页字段。
2. 引入 `EntrySource`、`ExploreEntry`、`ListPageSpec`，并把旧 search/explore 列表执行逻辑收敛到共享 runner。
3. 迁移 UI、doctor、tests 和 `fanqie-web.v2.json`。
4. 重新生成 schema，并更新文档示例。
5. 运行 Rust 测试、clippy、fmt、doc；因为 `Engine::explore/search` 会被 `tokio::spawn` 调用，还要运行 `cargo build` 验证 Future 仍为 `Send`。

## Open Questions

- `chain` 中某个动态源失败时，错误是否只进入日志/Warning，还是要阻断整个入口加载？倾向：保留已成功入口，同时把错误暴露给 UI。
- Fanqie 第一版是否只取 `label == "主分类"`，还是包括 `主题`、`角色`、`情节`？倾向：先全部纳入但不展开排序/状态/字数，若 UI 过长再收窄。
- `EntrySource::Fetch` 是否需要 render/intercept？Fanqie 分类 API 当前普通 HTTP 可用，但 schema 最好复用 `Request`，保留未来站点需要 render 的能力。
