## ADDED Requirements

### Requirement: 有界分页循环原语

`content` 与 `toc` SHALL 支持可选 `nextPage`(规则)与 `maxPages`(整数上限)。引擎抓取首页后,若 `nextPage` 求值得到**非空** URL 则继续抓取并合并结果,**直到 `nextPage` 为空或已达 `maxPages`**。`maxPages` 缺省时 MUST 应用一个内置硬上限(防御坏书源)。系统 MUST NOT 依赖从页面解析出的结束计数来决定终止。

#### Scenario: 多页正文循环到末页

- **WHEN** `content` 含 `value` 与 `nextPage`,某章正文分 3 页,第 3 页 `nextPage` 求值为空
- **THEN** 引擎抓取 3 页、合并正文,然后停止

#### Scenario: maxPages 封顶

- **WHEN** `nextPage` 始终返回非空(异常/坏源),`maxPages` 为 50
- **THEN** 引擎最多抓取 50 页即停止,不会无限循环

#### Scenario: 省略 nextPage 即单页

- **WHEN** `content` 只有 `value`、无 `nextPage`
- **THEN** 引擎只抓取单页(等价旧的单页正文)

### Requirement: 多页目录

`toc` SHALL 复用同一 `nextPage`+`maxPages` 语义,以支持分页/"加载更多"的目录页;跨页的章节与卷 MUST 按页序拼接为单一有序目录。

#### Scenario: 分页目录合并

- **WHEN** 目录分 N 页,`toc.nextPage` 指向下一页
- **THEN** 引擎按序抓取各页并合并为一份完整目录(卷/章顺序保持)
