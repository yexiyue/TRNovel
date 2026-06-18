## Context

主程序 UI 建在自研 `ratatui-kit` 0.6.0 上,事件靠**广播订阅** `use_events`(所有 handler 平等收到同一事件)+ 全局 `is_inputting: State<bool>` 手动门控来实现「输入框聚焦时背景不响应」。这套手做的互斥有个无法根除的**跨帧竞态**(`search_input.rs` 注释已吐槽):提交搜索的同一次 Enter,背景列表可能在分发中途看到 `is_inputting=false` 而误触「选中→进入阅读」。

`ratatui-kit` 0.7.0(2026-06-17)重写了事件系统(输入层栈 + 中央分发器,框架级互斥)、`element!` 去 sigil、并重写了内置组件体系——**内置组件吸取自 TRNovel 自己的组件**,是它们的上游演进版。本 change 升级到 0.7 并完成与「升级 + 事件 + 组件」强耦合的迁移。

调研事实(workflow 实测,2026-06-17):
- 8 个自定义组件 ↔ 0.7 内置 parity **全部 partial**,根因高度一致——内置把样式从 `theme.*` 解耦成裸 `Style`/`Color` props,且 `Select`/`MultiSelect` 无 loading/无滚动条/非虚拟化、`TreeSelect` 对目录也触发 `on_select`/无空态、`ShortcutInfoModal` 数据模型完全不同(40% 对齐)。
- 事件迁移面 21 文件;`is_inputting` 7 处 `use_context`;去 sigil 9×`#()` + 25×`$`。

约束:Edition 2024(Rust ≥1.89);`ratatui-kit` 升级**只动主程序 `src/`**,两个后端 crate(`parse-book-source`/`novel-tts`)不碰;主程序无单测,正确性靠 `cargo clippy` + **真跑 `cargo run`** 兜底。

## Goals / Non-Goals

**Goals:**
- 主程序编译并运行在 `ratatui-kit` 0.7.0 上,CI 四件套 + `cargo run` 首帧通过。
- 删除全局 `is_inputting`,输入互斥改由框架输入层承担,消除跨帧误触竞态(spec `tui-input-dispatch`)。
- 自定义组件拥抱 0.7 内置引擎,同时保留 TRNovel 的主题语义与内置缺失能力。
- 对终端用户**零可见行为变化**(键位/模态/选择/阅读流程逐项保持),输入竞态修复为唯一可观察差异。

**Non-Goals:**
- 全局 store → `Atom`(theme/novel_tts/browser_prompt + 干掉 OnceLock)——拆给 `global-state-to-atom`。本 change 后 store 仍是现有 `ContextProvider` 链(仅少一层 `is_inputting`)。
- `ratatui-kit` skill 优化与发布——拆给独立 workstream,靠本 change 的实战 gotcha 喂料。
- 新增任何用户可见功能 / 重新设计 UI。
- 升级 `ratatui`(已是 0.30.1 最新)及其它 `tui-*` 周边。

## Decisions

### D1:组件走「薄主题适配层」,而非调用点直接换内置

parity 全 partial 且根因是主题注入。三种方案:
- **(A) 调用点直接换内置**:删自定义组件,每个调用点(≈30 处)逐点传 `Style` props。→ 主题映射重复 30 遍、丢 loading/滚动条/虚拟化,回归面最大。✗
- **(B) 保留自定义只迁内部事件**:不碰内置。→ 放弃拥抱上游,与 0.7 持续漂移。✗
- **(C) 薄主题适配层**(选定):保留**项目组件名**作 wrapper,内部 `hooks.use_theme_config()` 把 `theme.*` 映射成内置 props 后委托 0.7 内置;内置缺的能力在 wrapper 内补。→ 调用点基本不动、主题映射每组件只写一次、拥抱内置引擎 + 保留项目语义。✓

```
调用点 (≈30 处,主要受 prop 改名影响)
      │  SearchInput / Select / ListSelect / ...  (项目组件名保留)
      ▼
项目 wrapper  ── use_theme_config() → theme.* 映射为 Style/Color props
      │        ── 补回内置缺失:loading / 空态 / 虚拟化 / on_select 过滤
      ▼
ratatui-kit 0.7 内置  (输入层 + use_event_handler 都封装在此)
```

这是用户决策 C(拥抱内置)与「保留自定义」的合题:拥抱内置引擎,保留项目主题语义。

### D2:`q`/`g`/`b` 留 `Current`(root 层),不设 `Global`

0.7 分发分两 phase:**Global phase 先跑且不受 `blocks_lower` 截断**,然后才是层内 phase。若把 `q`/`g`/`b` 设 `Global`,编辑搜索框键入 `q` 会被 Global 退出 handler 劫持——**文本输入失效**。正确映射:
- 背景 shell(`layout.rs` 的 `q`/`g`/`b`)与页面/Outlet 子树 handler → `EventScope::Current`(解析到 root 层)+ `Normal`,非自身键返回 `Ignored`。
- 输入框/独占模态 → `use_input_layer(open=active, blocks_lower=true)` 开层;层内 handler `High` + 处理后 `Consumed`。

效果:输入层活跃(blocks_lower)时,root 层全部 handler(含 `q`/`g`/`b` 与页面 `j`/`k`)被自动截断——**这正是 `is_inputting` 原本手做的事,现由层栈零竞态完成**。`Global` 仅留给 Resize 等必达事件。(注:框架已处理 Ctrl+C,紧急退出始终可用。)

### D3:`is_inputting` 按「门控 / 视觉态」二分拆解后删除

7 处 `use_context::<State<bool>>` 混了两种语义:
- **门控** `&& !is_inputting.get()`(~背景页面)→ 直接删,交 D2 的输入层截断。
- **视觉态** `is_editing: !is_inputting.get()` / `hide_cursor` / 边框高亮(SearchInput 同页子组件)→ 下沉为**该页局部 `use_state` 或 props**(谁拥有输入框谁知道自己在不在编辑,不需要全局广播)。

删除后 `src/app/mod.rs` 去掉该 `use_state` 与对应一层 `ContextProvider`。

### D4:组件分三档,内置缺失能力在 wrapper 内补回

| 档 | 组件 → 内置 | wrapper 补回的能力 |
|---|---|---|
| 🟢 易 | confirm→`ConfirmModal`、warning→`AlertModal` | 仅 `theme.*`→Style 映射;warning 另需 `is_error`→{标题/键/样式} 分支 |
| 🟡 中 | search_input→`SearchInput`、select→`Select`、file_select→`TreeSelect` | 主题映射;select 滚动条(`ScrollView` 包或接受去除);file_select `on_select` 过滤目录 + 空态 wrapper;prop 改名 |
| 🔴 难 | list_select→`Select`/`VirtualList`、multi_list_select→`MultiSelect` | 主题映射 + **保留 loading** + **保留虚拟化**(大列表用 `VirtualList`,小菜单用 `Select`)+ 逐项渲染回调 |

prop 改名集中:`is_editing`→`active`、`default_value`→`default_index`、`empty_message` `String`→`TextParagraph`。`SearchInput` 内置新增 `on_change`(每键回调),逐调用点评估是否需要(默认 `Handler::default()`)。

### D5:`shortcut_info_modal` 保留自定义,只迁内部事件

与内置 `ShortcutInfoModal` 仅 40% 对齐:自定义用扁平 `KeyShortcutInfo(Vec<(String,String)>)`,内置用分 `section` 的 `Vec<ShortcutInfoSection>`;关闭从「父 state toggle」变「`on_close` 回调」;布局不同;**9 个调用点**。全量重写 ROI 太低 → 保留自定义,仅把其内部(若有)`use_events`/相关键处理迁到 0.7,主题继续走 `use_theme_config()`。

### D6:去 sigil 机械化,唯一坑是链式 `widget()` 包裹

`#(expr)`→`{expr}`(含 `if/for/match/if let` 与纯表达式透传);`$expr`→`widget(expr)`、`$(w,s)`→`stateful(w,s)`。**坑**:`$Line::from("…").style(..).centered().right_aligned()` 这种链式,整条链都要进 `widget(...)`——`widget(Line::from("…")).style(..)` 是错的(`.style` 落到了 `widget()` 外)。17 处带链式,逐处确认包裹边界。

### D7:同层共享键须「仅 focused 子组件 Consumed」

`tts/settings.rs`(语速/音量/自动播放)与 `tts/voice_select.rs` 都在同层绑 `h`/`l`。广播模型下靠各自 `is_editing` 过滤;迁移后须保证只有 focused 子项 `Consumed`(其余 `Ignored`),否则一次 `h` 被多个子项响应。由父级 TTS 面板传 focused 标记给各子项,子项 handler 内 `if !focused { return Ignored }`。

## Risks / Trade-offs

- **[虚拟化:内置 `Select`/`MultiSelect` 非虚拟化]** 章节列表 / 搜索结果 / 书源列表可能很长,直接用内置会全量渲染 → 潜在卡顿。**Mitigation**:大列表的 wrapper 内部走 `VirtualList`(窗口化),小菜单走 `Select`;`VirtualList` 是手动驱动 Component 且 `on_select` 按 index 回调,wrapper 负责 index→item 还原。哪些列表算「大」见 Open Questions。
- **[薄适配层多一层间接]** wrapper 引入一层组件。**Mitigation**:函数组件透明布局,无额外布局节点;换来主题映射单点化,净收益为正。
- **[`SearchInput.on_change` 每键回调是新增语义]** 误接可能引入每键副作用。**Mitigation**:默认 `Handler::default()`,仅在确有需求的调用点接。
- **[无 UI 单测,正确性靠人肉]** 框架迁移最易回归。**Mitigation**:`cargo clippy -D warnings` + **强制 `cargo run` 走查关键流**(搜索/章节/TTS/模态/主题),每个 spec scenario 逐条手验。
- **[bump 后全量编译不过期]** 0.6→0.7 一改,sigil/事件/组件同时报错,中间态不可编译。**Mitigation**:按 Migration Plan 分步落、每步可 `cargo check`;必要时用 git 分支隔离,失败 `git revert` + 依赖钉回 0.6.0 即回滚。
- **[`tts` 同层 `h`/`l` 漏改 → 重复响应]** D7 若某子项忘加 focused 守卫即出 bug。**Mitigation**:tasks 把三个 tts 子项逐项列为勾选项 + 手验「只改当前项」。

## Migration Plan

分步落地(每步尽量保持 `cargo check` 可过,至少 step 1 完成后整体可过):

1. **bump + 去 sigil**:`Cargo.toml` 0.6→0.7;全量替换 `#()`→`{}`、`$`→`widget()`/`stateful()`(注意链式包裹)。这步让宏层先过。
2. **组件适配层(自底向上)**:先 🟢(confirm/warning),再 🟡(search/select/file),最后 🔴(list/multi,含虚拟化+loading)。每个 wrapper 内部同时把 `use_events`→`use_event_handler` + `use_input_layer`。
3. **页面事件迁移 + 删 is_inputting**:21 文件 `use_events`→`use_event_handler`,按 D2 映射 scope/priority;`is_inputting` 门控删、视觉态下沉;`app/mod.rs` 去掉 `is_inputting` provider。
4. **shortcut 内部迁移**(D5)。
5. **验证**:CI 四件套 + `cargo run` 逐 scenario 手验。

回滚:本 change 在独立分支;依赖钉回 `ratatui-kit = "0.6.0"` + `git revert` 即恢复。后端两 crate 不受影响。

## Open Questions

- **哪些列表用 `VirtualList` vs `Select`?** 候选大列表:章节目录(`select_chapter`)、网络搜索结果(`find_book`)、书源列表(`select_book_source`/`import_book_source`)、历史(`select_history`)。需在实现期按实际数据量定档(>一屏即倾向 `VirtualList`);小菜单(主页/主题项/TTS 设置)用 `Select`。
- **`SearchInput.on_change` 有调用点需要吗?** 现自定义无此回调;迁移时逐点确认是否有「输入即过滤/即校验」需求,否则留默认。
- **`select` 滚动条**:内置 `Select` 无滚动条,是用 `ScrollView` 包还是接受去除?取决于现有 select 是否真出现过超长选项(主题色选择可能)。实现期定。
