## ADDED Requirements

### Requirement: Explore entries use a two-stage model

系统 SHALL 将 explore 建模为入口加载与列表取页两个阶段。入口加载 SHALL 产生扁平的 `ExploreEntry` 列表，每个入口包含展示标题与变量集合；列表取页 SHALL 使用选中入口的变量、页码和页大小运行共享列表页规格。

#### Scenario: Explore entry variables drive page request

- **WHEN** 用户选择一个包含 `filter=audience1-cat2-262` 和 `sort=hottes` 的 explore entry，并请求第 2 页
- **THEN** 系统 SHALL 使用该入口变量与 `page=2` 渲染列表页请求模板，而不是从入口对象读取固定 URL

#### Scenario: Explore entries are flat UI choices

- **WHEN** 动态入口源返回多组远端分类数据
- **THEN** 系统 SHALL 将它们转换为 UI 可直接选择的扁平 `ExploreEntry` 列表

### Requirement: Entry sources compose as an ordered array

系统 SHALL 支持静态入口源与远端抓取入口源。`entries` SHALL 为入口源数组，系统 SHALL 按数组声明顺序合并各入口源产出的入口。系统 SHALL NOT 引入单独的链式组合（chain）类型；按序合并即由数组遍历表达。

#### Scenario: Static and dynamic entries are merged

- **WHEN** `entries` 数组包含一个 static source 和一个 fetch source
- **THEN** 系统 SHALL 按数组顺序返回 static source 生成的入口以及 fetch source 生成的入口

#### Scenario: Entry source can iterate request variables

- **WHEN** fetch entry source 配置 `forEach` 为 `gender=0` 与 `gender=1`
- **THEN** 系统 SHALL 分别以两组变量执行入口请求，并合并两个请求抽取出的入口

### Requirement: Fetch entry source flattens remote items into entry variables

系统 SHALL 允许 fetch entry source 使用 `list` 规则抽取远端数据项，并使用 `item.title` 与 `item.vars` 规则生成 `ExploreEntry`。`item.vars` 中的规则 MUST 能同时访问当前数据项和外层循环变量。

#### Scenario: Remote category item becomes an explore entry

- **WHEN** 远端分类项包含 `label=主分类`、`name=都市脑洞`、`category_id=262`，外层变量包含 `audience=男生` 与 `audienceToken=audience1`
- **THEN** 系统 SHALL 能生成标题类似 `男生·主分类·都市脑洞` 的入口，并生成包含 `audienceToken`、`categoryId`、`catGroup`、`sort` 等取页变量的变量集合

### Requirement: Search and explore share list page execution

系统 SHALL 使用同一套列表页规格执行 search 与 explore 的列表取数。列表页规格 SHALL 覆盖 prelude、request、list、item、hasMore 与 totalPages 行为。

#### Scenario: Explore list page uses render intercept

- **WHEN** explore page spec 配置 `request.render=true` 与 `request.interceptApi=book_list/v0`
- **THEN** 系统 SHALL 通过浏览器渲染并拦截匹配 API 响应来求值 `list` 与 `item` 规则

#### Scenario: Search list page reuses shared runner

- **WHEN** search 请求执行
- **THEN** 系统 SHALL 使用与 explore 相同的 list page runner 处理 request、list、item、hasMore 与 totalPages，而不是维护独立列表求值分支

### Requirement: Fanqie Web explore entries are dynamic

Fanqie Web 书源 SHALL 从 `category_list/v0` 动态加载 explore 分类入口，并继续通过 render/intercept 的 `book_list/v0` 获取所选入口的书籍列表。

#### Scenario: Fanqie categories load from category API

- **WHEN** 用户进入 Fanqie Web 书源的 explore 页面
- **THEN** 系统 SHALL 请求 Fanqie 分类 API，生成男生/女生分类入口，并在分类选择 UI 中展示这些入口

#### Scenario: Fanqie book list still uses signed browser path

- **WHEN** 用户选择 Fanqie 动态入口并请求书籍列表
- **THEN** 系统 SHALL 导航到由入口变量生成的 `/library/.../page_{{page}}?sort=...` URL，并拦截 `book_list/v0` 响应抽取书籍

### Requirement: Old explore category format is removed

系统 SHALL 不再接受旧的 `explore.categories` 加内联列表页字段作为有效书源格式。仓库内书源和生成 schema MUST 使用新的 `entries` 与 `page` 结构。

#### Scenario: Old explore format is rejected

- **WHEN** 书源配置仍使用旧的 `explore.categories` 和 `explore.list` 结构
- **THEN** 系统 SHALL 在配置解析阶段拒绝该书源，而不是静默转换
