use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    text::Line,
    widgets::Block,
};
use ratatui_kit::{components::tree_select::tree_item_at_position, prelude::*};
use std::{
    cell::RefCell,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    rc::Rc,
};
use tui_tree_widget::{TreeItem, TreeState};

use crate::theme::AppChromeTheme;

/// 滚轮一格滚动的行数,与常见终端编辑器的默认步长一致。
const WHEEL_LINES: usize = 3;

#[derive(Default, Props)]
pub struct FileSelectProps {
    pub items: Vec<TreeItem<'static, PathBuf>>,
    pub on_select: Handler<'static, PathBuf>,
    pub default_value: Option<usize>,
    pub top_title: Option<Line<'static>>,
    pub bottom_title: Option<Line<'static>>,
    pub is_editing: bool,
    pub empty_message: String,
    /// 树内容变化后是否展开全部目录。筛选态必须传 `true`:命中的文件多半埋在
    /// 子目录里,保持折叠等于让用户逐层翻找,搜索结果就白筛了。
    pub expand_all: bool,
}

#[component]
pub fn FileSelect(props: &mut FileSelectProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let state = hooks.use_state(TreeState::default);
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let page_lines = usize::from(hooks.use_previous_size().height.saturating_sub(2)).max(1);

    let is_empty = props.items.is_empty();
    // 「树变了」用指纹表达,而不是收集全部路径:后者每帧都要递归分配一个
    // Vec<PathBuf>(渲染本身已经 clone 过一次整棵树了),指纹只遍历不分配。
    let fingerprint = tree_fingerprint(&props.items);
    let expand_all = props.expand_all;

    // 搜索或目录扫描结果变化后,旧选中路径可能已经不在当前树中。清空选中,
    // 避免下一次 Enter 沿用被筛掉的旧文件。use_effect 是同步执行的(就在渲染
    // 体内),所以渲染出去的那一帧选中一定属于当前这棵树,取项处无需二次校验。
    hooks.use_effect(
        || {
            let mut state = state.write();
            state.select(Vec::new());
            state.close_all();
            if expand_all {
                for branch in collect_branches(&props.items) {
                    state.open(branch);
                }
            }
        },
        (fingerprint, expand_all),
    );

    // 键盘与鼠标两个闭包共享同一个回调(Handler 不 Clone,经 Rc<RefCell> 转发)。
    let on_select = Rc::new(RefCell::new(props.on_select.take()));

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let is_editing = props.is_editing;
        let on_select = on_select.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press || !is_editing {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Char('h') | KeyCode::Left => {
                    state.write().key_left();
                    EventResult::Consumed
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    state.write().key_down();
                    EventResult::Consumed
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    state.write().key_up();
                    EventResult::Consumed
                }
                KeyCode::PageUp => {
                    state
                        .write()
                        .select_relative(|current| current.unwrap_or(0).saturating_sub(page_lines));
                    EventResult::Consumed
                }
                KeyCode::PageDown => {
                    state.write().select_relative(|current| {
                        current.map_or(0, |index| index.saturating_add(page_lines))
                    });
                    EventResult::Consumed
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    let res: Option<PathBuf> = state.read().selected().last().cloned();
                    if let Some(path) = res {
                        state.write().toggle_selected();
                        if path.is_file() {
                            (on_select.borrow_mut())(path);
                        }
                    }
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        }
    });

    // 鼠标:滚轮滚动视口(树行数多,滚视口比逐项移动光标更符合直觉);
    // 点击 = 选中节点,点已选中的目录 = 切换展开,点文件 = 直接打开(on_select)。
    // 精确命中节点的可见行,排除边框及视口尾部空白。
    hooks.use_pointer(PointerOptions::default(), {
        let is_editing = props.is_editing;
        let on_select = on_select.clone();
        let items = props.items.clone();
        move |ev| {
            if !is_editing {
                return EventResult::Ignored;
            }
            match ev.kind {
                PointerKind::ScrollUp => {
                    state.write().scroll_up(WHEEL_LINES);
                    EventResult::Consumed
                }
                PointerKind::ScrollDown => {
                    state.write().scroll_down(WHEEL_LINES);
                    EventResult::Consumed
                }
                PointerKind::Click { .. } => {
                    let inner = Block::bordered().inner(ev.area);
                    let hit = tree_item_at_position(&state.read(), &items, inner, ev.position)
                        .and_then(|path| path.last().cloned());
                    match hit {
                        Some(path) => {
                            state.write().click_at(ev.position);
                            if path.is_file() {
                                (on_select.borrow_mut())(path);
                            }
                            EventResult::Consumed
                        }
                        None => EventResult::Ignored,
                    }
                }
                _ => EventResult::Ignored,
            }
        }
    });

    let mut border = Block::bordered().border_style(theme.border);

    if let Some(title) = props.top_title.clone() {
        border = border.title_top(title);
    }
    if let Some(title) = props.bottom_title.clone() {
        border = border.title_bottom(title);
    }

    if is_empty {
        return element!(
            Border(
                top_title: props.top_title.clone(),
                bottom_title: props.bottom_title.clone(),
                border_style: theme.border,
            ){
                Center(
                    height:Constraint::Length(5),
                    width:Constraint::Percentage(50)
                ){
                    Text(
                        text: props.empty_message.clone(),
                        alignment: Alignment::Center,
                        style: theme.empty,
                        wrap: true,
                    )
                }
            }
        )
        .into_any();
    }

    element!(TreeSelect<PathBuf>(
        // 文件导航/确认由业务处理;框架仍负责渲染和悬停。
        active: false,
        style: theme.text,
        highlight_style: theme.selected,
        // 同 select_chapter:默认 surface 底色在本应用 palette 下不可见,显式指定。
        hover_style: theme.highlight,
        state: state,
        items: props.items.clone(),
        scrollbar: crate::theme::scrollbar_style(theme.border, theme.highlight),
        block: border,
    ))
    .into_any()
}

/// 树内容的指纹,用作「树变了」的 effect deps。
///
/// 只遍历不分配 —— 每帧都要算,不能像收集全部路径那样每次递归建一个 Vec。
fn tree_fingerprint(items: &[TreeItem<'static, PathBuf>]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_items(items, &mut hasher);
    hasher.finish()
}

fn hash_items(items: &[TreeItem<'static, PathBuf>], hasher: &mut DefaultHasher) {
    items.len().hash(hasher);
    for item in items {
        item.identifier().hash(hasher);
        hash_items(item.children(), hasher);
    }
}

/// 收集所有含子节点的分支,每项是从根到该节点的完整路径 —— `TreeState::open`
/// 要的就是完整路径,只给节点自身的 identifier 展不开嵌套目录。
fn collect_branches(items: &[TreeItem<'static, PathBuf>]) -> Vec<Vec<PathBuf>> {
    let mut branches = Vec::new();
    push_branches(items, &mut Vec::new(), &mut branches);
    branches
}

fn push_branches(
    items: &[TreeItem<'static, PathBuf>],
    prefix: &mut Vec<PathBuf>,
    branches: &mut Vec<Vec<PathBuf>>,
) {
    for item in items {
        if item.children().is_empty() {
            continue;
        }
        prefix.push(item.identifier().clone());
        branches.push(prefix.clone());
        push_branches(item.children(), prefix, branches);
        prefix.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str) -> TreeItem<'static, PathBuf> {
        TreeItem::new_leaf(PathBuf::from(name), name.to_owned())
    }

    fn dir(name: &str, children: Vec<TreeItem<'static, PathBuf>>) -> TreeItem<'static, PathBuf> {
        TreeItem::new(PathBuf::from(name), name.to_owned(), children).unwrap()
    }

    /// 指纹是重置选中的唯一触发条件:筛选前后必须不同,否则旧选中会留到新树上,
    /// Enter 就打开了一个已经被筛掉的文件(issue #64 的回归点)。
    #[test]
    fn fingerprint_changes_when_filtering_removes_items() {
        let all = vec![leaf("A.txt"), leaf("B.txt")];
        let filtered = vec![leaf("A.txt")];

        assert_ne!(tree_fingerprint(&all), tree_fingerprint(&filtered));
        assert_eq!(tree_fingerprint(&all), tree_fingerprint(&all.clone()));
    }

    /// 同名文件换了层级也算树变了 —— 只比较扁平集合会漏掉这种情况。
    #[test]
    fn fingerprint_is_sensitive_to_nesting() {
        let flat = vec![leaf("A.txt")];
        let nested = vec![dir("sub", vec![leaf("A.txt")])];

        assert_ne!(tree_fingerprint(&flat), tree_fingerprint(&nested));
    }

    /// 展开用的是完整路径,且叶子不进结果:给 `open` 传叶子路径是无效调用。
    #[test]
    fn branches_are_full_paths_and_exclude_leaves() {
        let items = vec![dir("a", vec![dir("b", vec![leaf("c.txt")])]), leaf("d.txt")];

        assert_eq!(
            collect_branches(&items),
            vec![
                vec![PathBuf::from("a")],
                vec![PathBuf::from("a"), PathBuf::from("b")],
            ]
        );
    }
}
