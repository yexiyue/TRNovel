# Design: reader-paging-preferences

## 1. 滚动坐标系（本变更的核心）

### 1.1 现状与问题

```rust
// read_content.rs（旧）
let line_count = paragraph.line_count(width - 2)
    .saturating_sub(height - 3);          // 变量名叫 line_count，实为 max_scroll
let current_line = round(line_percent * line_count);
let page_lines  = (height - 3).max(1);
```

一个变量 `line_count` 同时承担三个语义：**滚动上限**、**进度百分比的分母**、**底部「n/m 行」显示的分母**。三者被迫共享「贴底钳位」这一个假设，于是：

- 想让翻页在末尾留白 → 必须抬高滚动上限 → 进度百分比的分母跟着变 → 存量进度漂移；
- 百分比分母含视口高度 → 换终端尺寸后同一个百分比指向不同的行；
- 底部显示的 `m` 不是总行数，而是「总行数 − 视口高度」，本身就是个没有阅读含义的数。

### 1.2 新坐标系：三个量各司其职

```
total       = paragraph.line_count(width - 2)      内容总行数（唯一的绝对基准）
view        = (height - 3).max(1)                  可见行数（上下边框 + 底部状态栏）
end_scroll  = total.saturating_sub(view)           「贴底位置」——最后一行正好落在屏幕底部
step        = view.saturating_sub(overlap).max(1)  翻页步长
```

三条派生规则：

| 用途 | 公式 | 说明 |
|---|---|---|
| 章末判定 | `at_end = current_line >= end_scroll` | 「最后一行已可见」= 读完本章，与旧行为逐帧一致 |
| 落盘进度 | `line_percent = current_line / total` | **分母是 total，与视口无关** |
| 恢复位置 | `current_line = round(percent × total)` | `percent >= 1.0` 特判为 `end_scroll`（见 1.4） |

滚动条与底部行号一并改用 `total` 作分母，显示的 `m` 从此就是真正的总行数。

### 1.3 末尾留白为什么不需要额外的上限常量

看似需要一个 `max_scroll = total - 1` 来防止滚过头，实际不需要——**边界判定先于滚动生效**：

```
PageDown:  if at_end { 走边界武装/翻章 } else { current_line += step }
```

`current_line < end_scroll` 是执行 `+= step` 的前提，故落点上界为

```
(end_scroll - 1) + step  ≤  (total - view - 1) + view  =  total - 1
```

**天然不越界**，最后一行必定仍在屏幕内。少一个常量、少一处 clamp、少一个不变量要维护。

留白量的直观形态（total=100, view=30, overlap=2, step=28）：

```
current_line:  0 ──▶ 28 ──▶ 56 ──▶ 84        end_scroll = 70
                                    ▲
                                    └─ 84 > 70：屏幕显示 84..99 共 16 行 + 14 行留白，
                                       且 at_end 已成立 → 再按一次进入「章末武装」

每一步都是 28 行，步长绝对一致 ✔
```

### 1.4 「章末」是意图，不是比例 —— 用 `ScrollTarget` 而非哨兵值

`GoBottom`（End 键）与「顶部 ↑ 翻回上一章落到章末」发出的是**意图**，而落盘写回的是**位置**。最初两者共用一个 `State<f64>`，靠 `percent >= 1.0` 这个取值区间来区分——需要用散文论证「用户写回的值必然 < 1.0，撞不上这一支」的不变量，正是类型没有承担职责的信号：该不变量横跨三处表达式（写回的分母、`current_line += delta` 的上界推导、memo 的 `.min(total-1)`），任何一处改动都会静默破坏它。

故把通道拆成两个变体：

```rust
enum ScrollTarget {
    Ratio(f64),    // 精确位置，r = current_line / total
    ChapterEnd,    // 「读完本章」的意图，由 ReadContent 解析为 end_scroll
}
```

`resolve(total, end_scroll)` 是唯一的解析点。f64 只在**持久化边界**转换（`from_ratio` / `as_ratio`，`ChapterEnd` 编码为 1.0）——那时它只是编码，不再是运行期哨兵。

**`Ratio` 解析必须钳到 `end_scroll`**（code review 发现）：比例与视口无关，换终端尺寸或宽度重排后 `ratio * total` 可能落在贴底位置**之下**——屏幕停在「最后一屏再往下」，`current_line < end_scroll` 恒假使前向滚动/翻页当场失效、误报章末，连按两下还会跳章。旧公式的值域天然 ⊆ `[0, end_scroll]`，新坐标系没有这个不变量，必须显式钳。

于是留白需要第三个变体：

```rust
Overscroll { ratio: f64, end_scroll: usize }   // 记下产生它的视口
```

**留白是视口局部的视觉状态**——留白多少行由当时的 `total`/`view` 决定，视口一变就失去意义。`resolve` 里 `end_scroll` 不匹配即退化为贴底；落盘只存比例，重开回到合法位置。这条区分正是「章内翻页要允许越过贴底」与「恢复进度不得越过贴底」两个需求的分界，同一条钳位规则无法兼顾。

### 1.5 破坏性与取舍

存量 `line_percent` 的分母从 `total - view` 变为 `total`，同一个值指向的行会前移约 `percent × view` 行（最多约一屏）。已确认接受：换来的是**跨终端尺寸稳定的进度**，这个缺陷不修就会永远存在，且越晚修存量越大。不做迁移脚本——迁移需要知道存档写入时的视口高度，而那从未被记录。

## 2. 翻页与滚动合并为一对分支

`ScrollDown` 与 `PageDown` 的差异只有步长，边界行为（全书末章只提示不武装 / 章末武装 / 二次确认翻章）应当完全相同。与其复制一份，不如合并：

```rust
ReaderAction::ScrollDown | ReaderAction::PageDown => {
    let delta = if action == ReaderAction::PageDown { step } else { 1 };
    if current_line < end_scroll {
        current_line = (current_line + delta).min(...);   // 见 1.3：无需额外钳位
        ...
    } else if !has_next { edge.set(Edge::AtLast) }
      else if edge.get() == Edge::Next { edge.set(Edge::None); on_next(()) }
      else { edge.set(Edge::Next) }
}
```

`ScrollUp | PageUp` 同构。收益：边界语义**单一实现**，四个分支缩为两个，新增第三种步长（若将来加半页翻页）只需多一个 `delta` 取值，不再复制边界逻辑（OCP）。

> 注：合并后 `PageUp`/`PageDown` 自动获得「章首/章末二次确认」，这正是本变更要补的行为；显式 `←`/`→` 翻章仍走各自分支、立即翻章不武装（保留快路）。

## 3. 配置归属：不新建任何设施

现有四层配置各有归属，`page_overlap` 属于「阅读行为偏好」，与 `show_title` 同宗：

```
~/.novel/
├── appearance.json      → Atom APPEARANCE      主题/背景（主题设置页可视化）
├── reader-display.json  → Atom READER_DISPLAY  ← page_overlap 落在这里
├── keybindings.toml     → Atom KEYMAP          键位（只读，手改）
└── tts_config.json      → State<TTSConfig>     听书（TTS 浮层可视化）
```

不新建配置文件、不新建 atom、不新建 Provider。`ReadContent` 已持有 `READER_DISPLAY` 句柄（`show_title` 在用），读 `page_overlap` 零额外接线。

**取值钳位放在配置类型内**（`page_overlap()` 访问器返回钳位后的值），而非每个调用点各钳一次：手改 JSON 写入 `999` 或负数时由类型自己兜底，调用方无需知道范围。上限 10——再大就会让「翻一页」退化成「滚几行」，属于配置错误而非偏好。

## 4. 阅读设置浮层

### 4.1 为什么是浮层而不是设置页

| 方案 | 否决理由 |
|---|---|
| 塞进 TTS 面板 | 语义错位，那是「听书设置」 |
| 裸键位 `[`/`]` 直接加减 | 吃两个键位、不可发现、只能承载一个标量 |
| 首页「主题设置」升级为通用设置页 | 离阅读现场远，调完看不到效果；主题页是带配色预览的专用列表，改通用页是不成比例的重构 |

浮层就地调、关掉即可试，且 `ShortcutInfoModal` 是现成的可发现性入口。面板同时收纳 `show_title`——它此前只有 `v` 键盲切、零可视入口，两项同属 `ReaderDisplayConfig`、落同一个文件，内聚。

### 4.2 结构与复用：上提的是**按键协议**，不只是边框

`SettingItem`（边框 + 编辑态高亮）本身与 TTS 无关，上提到 `src/components/setting_item.rs`。但只提外壳是提错了东西——真正会漂移、真正需要「只有一处」的是**行为**：全仓当时有 5 处逐字相同的「`Event::Key` 解构 → `KeyEventKind::Press` → `if !is_editing { Ignored }` → `Left|h` 减 / `Right|l` 加」，而「同层多个条目都收到这些键，只有聚焦者可消费」这条约定被复制了 5 份，漏写一处就是一次按键被多个条目同时响应。开关型条目的取值展示已经开始漂移（`开/关` vs `true/false`）。

故分两层：

- `SettingItem` —— 纯容器，供交互特殊的条目用（下载进度的 Enter/Esc、音色选择的列表导航）。
- `AdjustableSettingItem` —— 容器 + 「←/→ 调整」协议（`label` / `value` / `on_decrease` / `on_increase`）。

阅读设置的两项与 TTS 的速度/音量/自动播放共 5 个调用点全部退化为纯数据，各约 8 行。将来把面板键位接入 keymap 体系时也只需改一处。

**步进与边界同样收进配置类型**：`ReaderDisplayConfig::increase_page_overlap()` / `decrease_page_overlap()` / `page_step(view)`，与仓库既有的 `TTSConfig::increase_speed()` 一致。UI 不再知道上界，`PAGE_OVERLAP_MAX` 得以私有；磁盘脏值在 `load()` 归一（读侧兜底会让越界值永远留在文件里）。

面板形态与 `TTSManager` 一致（`Modal` + `ScrollView` + `index` 选中 + 逐项 `is_editing` 门控），沿用其已验证的约定：

- `blocks_lower: false` —— 否则截断 root 层 handler，`o` 键关不掉面板、`Tab`/`i` 一并失灵（知识库记录的既有坑）。
- 背景正文用 `is_scroll` 门控，面板开启时不抢键。
- 每个条目内部 `if !is_editing { return Ignored }`，否则一次 `←` 被多个条目同时响应。

### 4.3 派生值同屏展示

条目显示 `翻页重叠: 2 行（每页滚动 28 行）`。用户无法凭「2」想象效果，但看到实际步长就懂了；且当终端极矮、`overlap` 把步长压到 1 时，屏幕上直接可见，无需另设提示。零成本的可解释性。

### 4.4 落盘时机

`←`/`→` 连按会连续触发写盘。用既有 `use_debounce_effect` 以 `page_overlap` 为 deps 延迟落盘，保持「即改即存」的心智又不写穿磁盘。`show_title` 的布尔切换不频繁，但走同一路径以免两套时序。

### 4.5 与 TTS 浮层互斥：用一个状态，不是两个 bool

两个浮层都是同尺寸 `Modal`，同时打开会叠加渲染且都监听 `←/→`。用两个独立 bool 表达就得在每个入口手写「关掉另一个」——pairwise 接线随面板数平方增长，且 `is_scroll` 会退化成三重取反、帮助内容变成 if/else-if 链，漏改任意一处都是静默的状态错位。

改用 `enum Panel { None, Tts, ReaderSettings }` 单一状态：**互斥由类型保证**（不可能同时为两个值），`is_scroll = panel == None && !info_modal_open`，两个入口 arm 合并成一支按 `target` 取值的实现。`is_read_mode` 那段纠缠（目录模式按入口键先切模式）与「哪个面板」正交，原样保留。`info_modal_open` 保持独立——它是叠在面板之上的层，不是同层互斥项。

## 5. 默认值决策

`page_overlap` 默认 **2**（issue 建议默认 0 以保持旧行为）。理由：默认 0 意味着绝大多数用户永远不会发现这个功能，而重叠 2 行是 Vim `Ctrl-F`、`less`、多数阅读器的通行默认，对所有用户都是净改进。本变更已确认可破坏兼容性，不追求逐帧一致。想要旧行为的用户把它调成 0 即可——面板里一眼能找到。

`ctrl-f` / `ctrl-b` 补进默认键位表：与 `page_up`/`page_down` 的整页语义精确对应。**不**绑 `ctrl-d`/`ctrl-u`——它们在 Vim 里是半页，绑到整页 action 上是错的语义；要支持得另立半页 action，超出本次范围。
