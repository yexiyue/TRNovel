//! 本地 TXT 目录（章节 + 分卷）检测引擎。
//!
//! 取代原先单条硬编码正则 `第.+章`。核心思路（参考 Legado `TxtTocRule` 与 kaf-cli）：
//!
//! - **规则集**：一组可启用的正则规则，分为「章节规则」「卷规则」「排除规则」。
//!   用 [`fancy_regex`] 编译，因此用户可直接使用含前后向断言的 Legado 规则。
//! - **逐行启发式**：每行先清洗（行尾去全部空白；行首仅去 ASCII 空白、保留全角缩进 `　`）、
//!   跳空行、限制标题字符数、排除表过滤、卷优先于章、并丢弃以句号结尾的行（正文特征）。
//! - **多规则竞争**：当存在多条章节规则时，按「有效命中数」（相邻命中间隔足够大）择优，
//!   自动适配「章 / 节 / 回」等不同计数词的书。
//! - **可配置**：内置默认规则集作兜底；若 `~/.novel/toc_rules.json` 存在则合并用户规则，
//!   解析失败时安全回退到默认（呼应 issue #49）。

use crate::novel::VolumeMarker;
use crate::utils::novel_catch_dir;
use serde::{Deserialize, Serialize};

/// 默认标题最大字符数（超过则视为正文）。
const DEFAULT_MAX_TITLE_LEN: usize = 35;

/// 多规则竞争评分时，相邻被计数命中的最小字节间隔（仅用于打分，不影响最终章节列表）。
const SCORE_MIN_GAP: usize = 64;

/// 单条目录规则，字段对齐 Legado `TxtTocRule`（额外增加 `is_exclude`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TocRule {
    /// 规则名称（仅用于展示/调试）。
    pub name: String,
    /// 正则字符串（`fancy_regex` 语法，支持断言）。
    pub rule: String,
    /// 是否为卷规则。
    #[serde(default)]
    pub is_volume: bool,
    /// 是否为排除规则（命中即丢弃该行，优先于卷/章判定）。
    #[serde(default)]
    pub is_exclude: bool,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enable: bool,
    /// 示例标题（可选）。
    #[serde(default)]
    pub example: Option<String>,
    /// 排序号（可选）。
    #[serde(default)]
    pub serial_number: i32,
}

fn default_true() -> bool {
    true
}

/// 目录规则集（含最大标题长度与全部规则）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TocRuleSet {
    /// 标题最大字符数。
    #[serde(default = "default_max_title_len")]
    pub max_title_len: usize,
    /// 规则列表（章节 / 卷 / 排除混合）。
    pub rules: Vec<TocRule>,
}

fn default_max_title_len() -> usize {
    DEFAULT_MAX_TITLE_LEN
}

impl Default for TocRuleSet {
    fn default() -> Self {
        Self::builtin()
    }
}

impl TocRuleSet {
    /// 内置默认规则集（编译进二进制，作为兜底）。
    pub fn builtin() -> Self {
        let rule = |name: &str, pat: &str, is_volume: bool, is_exclude: bool, sn: i32| TocRule {
            name: name.to_string(),
            rule: pat.to_string(),
            is_volume,
            is_exclude,
            enable: true,
            example: None,
            serial_number: sn,
        };

        // 中文数字 + 阿拉伯数字字符类（复用于卷与章）。
        const NUM: &str = r"[0-9〇零一二两三四五六七八九十百千万壹贰叁肆伍陆柒捌玖拾佰仟]";

        Self {
            max_title_len: DEFAULT_MAX_TITLE_LEN,
            rules: vec![
                // —— 排除规则（最先判定）——
                rule(
                    "排除:卷计数词歧义",
                    &format!(r"^第?{NUM}{{1,8}}(?:部门|部队|部属|部分|部件|部落)"),
                    false,
                    true,
                    -2,
                ),
                rule(
                    "排除:节课",
                    &format!(r"^第{NUM}{{1,8}}节课"),
                    false,
                    true,
                    -1,
                ),
                // —— 卷规则（优先于章）——
                rule(
                    "卷/部/篇",
                    &format!(r"^第{NUM}{{1,8}}[卷部篇]"),
                    true,
                    false,
                    0,
                ),
                rule("上中下卷", r"^[上中下][卷部篇]", true, false, 1),
                // —— 章节规则 ——
                // 计数词后必须是「行尾」或「分隔符+标题」，避免「第一回合」「第三话说」等
                // 计数词后紧跟汉字的正文被误判。
                rule(
                    "数字章节(章/节/回/话)",
                    &format!(r"^第{NUM}{{1,12}}[章节回话](?:[ 　\t、，,:：．.\-—_~·].*)?$"),
                    false,
                    false,
                    2,
                ),
                rule(
                    "英文章节",
                    r"^(?:[Cc]hapter|[Ss]ection|[Pp]art|[Ee]pisode)\s*\d{1,4}",
                    false,
                    false,
                    3,
                ),
                rule(
                    "特殊章节",
                    r"^(?:楔子|引子|序章|序言|前言|后记|尾声|终章|完本感言|番外|外传|附录|内容简介|作品相关)",
                    false,
                    false,
                    4,
                ),
            ],
        }
    }

    /// 加载规则集：内置默认 + 合并 `~/.novel/toc_rules.json` 用户规则。
    ///
    /// 配置文件缺失或解析失败时安全回退到内置默认，绝不 panic。
    pub fn load() -> Self {
        let mut set = Self::builtin();

        let Ok(dir) = novel_catch_dir() else {
            return set;
        };
        let path = dir.join("toc_rules.json");
        if let Ok(content) = std::fs::read_to_string(&path) {
            set.merge_user_json(&content);
        }
        set
    }

    /// 把用户配置 JSON 合并进当前规则集：覆盖 `max_title_len`、追加 `rules`。
    ///
    /// 解析失败时保持当前（默认）规则不变，返回 `false`。
    pub fn merge_user_json(&mut self, content: &str) -> bool {
        match serde_json::from_str::<TocRuleSet>(content) {
            Ok(user) => {
                self.max_title_len = user.max_title_len;
                self.rules.extend(user.rules);
                true
            }
            Err(_) => false,
        }
    }
}

/// 已编译的规则。
struct CompiledRule {
    regex: fancy_regex::Regex,
    is_volume: bool,
    is_exclude: bool,
}

fn compile(set: &TocRuleSet) -> Vec<CompiledRule> {
    set.rules
        .iter()
        .filter(|r| r.enable)
        // 编译失败（用户写了非法正则）直接跳过该条，保证不崩溃
        .filter_map(|r| {
            fancy_regex::Regex::new(&r.rule)
                .ok()
                .map(|regex| CompiledRule {
                    regex,
                    is_volume: r.is_volume,
                    is_exclude: r.is_exclude,
                })
        })
        .collect()
}

/// 清洗一行用于匹配：
///
/// - 行尾去除所有空白（含全角空格 `　`、`\r`、`\n`）。
/// - 行首仅去除 ASCII 空格/Tab，**保留全角空格 `　`**。
///
/// 因为中文排版里段落用全角空格 `　　` 缩进，而章节标题通常顶格；保留行首全角空格可
/// 让锚定正则 `^第…` 自然拒绝「正文中缩进且以第X章开头」的伪标题。
fn clean_line(line: &str) -> &str {
    line.trim_end().trim_start_matches([' ', '\t'])
}

/// 判断一行（已 [`clean_line`] 清洗）是否应作为标题候选。
fn is_title_candidate(cleaned: &str, max_title_len: usize) -> bool {
    if cleaned.is_empty() {
        return false;
    }
    if cleaned.chars().count() > max_title_len {
        return false;
    }
    // 正文特征：以句号结尾（不排除 ！？ 因为部分章节标题会以其结尾）。
    !matches!(cleaned.chars().last(), Some('。' | '．' | '.'))
}

/// 从「(整行, 字节偏移)」序列检测目录，返回 `(扁平章节列表, 卷元数据)`。
///
/// - 章节项为 `(标题, 字节偏移)`，与 `LocalNovel` 的 `Chapter` 类型一致。
/// - 卷的 `first_chapter_index` 指向其后第一章在扁平列表中的索引。
pub fn detect<I>(lines: I, set: &TocRuleSet) -> (Vec<(String, usize)>, Vec<VolumeMarker>)
where
    I: IntoIterator<Item = (String, usize)>,
{
    let compiled = compile(set);
    let chapter_rules: Vec<&CompiledRule> = compiled
        .iter()
        .filter(|r| !r.is_volume && !r.is_exclude)
        .collect();

    // 每条章节规则各自累积命中 (标题, 偏移)。
    let mut per_rule: Vec<Vec<(String, usize)>> = vec![Vec::new(); chapter_rules.len()];
    let mut volume_hits: Vec<(String, usize)> = Vec::new();

    for (line, offset) in lines {
        let cleaned = clean_line(&line);
        if !is_title_candidate(cleaned, set.max_title_len) {
            continue;
        }
        // 排除优先
        if compiled
            .iter()
            .any(|r| r.is_exclude && r.regex.is_match(cleaned).unwrap_or(false))
        {
            continue;
        }
        // 卷优先于章
        if compiled
            .iter()
            .any(|r| r.is_volume && r.regex.is_match(cleaned).unwrap_or(false))
        {
            volume_hits.push((cleaned.to_string(), offset));
            continue;
        }
        // 章节：记录到每条匹配的规则
        for (i, cr) in chapter_rules.iter().enumerate() {
            if cr.regex.is_match(cleaned).unwrap_or(false) {
                per_rule[i].push((cleaned.to_string(), offset));
            }
        }
    }

    // 多规则竞争：选有效命中数最多的章节规则。
    let best = per_rule
        .into_iter()
        .max_by_key(|hits| effective_count(hits))
        .unwrap_or_default();

    let chapters = best;

    // 计算每个卷的首章索引（章节已按偏移升序）。
    let volumes = volume_hits
        .into_iter()
        .map(|(title, offset)| VolumeMarker {
            title,
            first_chapter_index: chapters.partition_point(|(_, o)| *o < offset),
        })
        .collect();

    (chapters, volumes)
}

/// 有效命中数：仅统计与上一个被计数命中的字节间隔 >= [`SCORE_MIN_GAP`] 的命中，
/// 用于抑制连续误报刷高某条劣质规则的得分。
fn effective_count(hits: &[(String, usize)]) -> usize {
    let mut count = 0usize;
    let mut last: Option<usize> = None;
    for (_, offset) in hits {
        match last {
            Some(prev) if offset.saturating_sub(prev) < SCORE_MIN_GAP => {}
            _ => {
                count += 1;
                last = Some(*offset);
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(input: &str) -> Vec<(String, usize)> {
        // 模拟逐行读取：记录每行起始字节偏移（含行尾换行符）。
        let mut out = Vec::new();
        let mut offset = 0usize;
        for line in input.split_inclusive('\n') {
            out.push((line.to_string(), offset));
            offset += line.len();
        }
        out
    }

    #[test]
    fn detects_chapter_with_zhang() {
        let text = "第1章 罗峰\n正文内容。\n第2章 RR\n更多正文。\n";
        let (chapters, volumes) = detect(lines(text), &TocRuleSet::builtin());
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].0, "第1章 罗峰");
        assert!(volumes.is_empty());
    }

    #[test]
    fn detects_chapter_with_jie() {
        // 章节单位为「节」，应被识别（旧正则 `第.+章` 会整本归零）。
        let text = "第一节 纵身亡魔心仍不悔\n正文。\n第二节 逆光阴五百年觉悟\n正文。\n";
        let (chapters, _) = detect(lines(text), &TocRuleSet::builtin());
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[1].0, "第二节 逆光阴五百年觉悟");
    }

    #[test]
    fn volume_takes_priority_and_indexes_first_chapter() {
        let text = "第一卷 魔性不改\n第一节 甲\n正文。\n第二节 乙\n第二卷 魔子出山\n第三节 丙\n";
        let (chapters, volumes) = detect(lines(text), &TocRuleSet::builtin());
        assert_eq!(chapters.len(), 3);
        assert_eq!(volumes.len(), 2);
        assert_eq!(volumes[0].first_chapter_index, 0); // 第一卷 → 第一节
        assert_eq!(volumes[1].first_chapter_index, 2); // 第二卷 → 第三节(索引2)
    }

    #[test]
    fn ignores_inline_chapter_references() {
        // 正文中引用「第三章」不应被误判为标题（行首非「第」/超长/缩进）。
        let text = "　　违反了我们商家城的城规第三章第二十五条，必须严惩。\n　　（详情见本卷第672章。）\n第一章 真标题\n";
        let (chapters, _) = detect(lines(text), &TocRuleSet::builtin());
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].0, "第一章 真标题");
    }

    #[test]
    fn excludes_counter_word_ambiguity() {
        // 「第三部分」是名词不是卷；「第三节课」不是章。
        let text = "第三部分 概述\n第三节课的内容。\n第一章 正章\n";
        let (chapters, volumes) = detect(lines(text), &TocRuleSet::builtin());
        assert!(volumes.is_empty());
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].0, "第一章 正章");
    }

    #[test]
    fn long_line_is_not_title() {
        let long = "第一章".to_string() + &"超长内容".repeat(20) + "\n";
        let (chapters, _) = detect(lines(&long), &TocRuleSet::builtin());
        assert!(chapters.is_empty());
    }

    #[test]
    fn merge_valid_user_json_adds_rule() {
        let mut set = TocRuleSet::builtin();
        let builtin_len = set.rules.len();
        let json = r#"{ "maxTitleLen": 50, "rules": [
            { "name": "自定义", "rule": "^卷[一二三]", "isVolume": true }
        ] }"#;
        assert!(set.merge_user_json(json));
        assert_eq!(set.max_title_len, 50);
        assert_eq!(set.rules.len(), builtin_len + 1);
    }

    #[test]
    fn merge_corrupt_user_json_falls_back() {
        // 损坏的 JSON 应安全回退：保持内置默认不变。
        let mut set = TocRuleSet::builtin();
        let builtin_len = set.rules.len();
        assert!(!set.merge_user_json("{ this is not valid json "));
        assert_eq!(set.rules.len(), builtin_len);
        // 回退后仍能正常检测
        let (chapters, _) = detect(lines("第1章 甲\n正文。\n"), &set);
        assert_eq!(chapters.len(), 1);
    }

    #[test]
    fn corrupt_user_rule_does_not_crash() {
        // 含非法正则的规则应被跳过，不影响其余规则。
        let mut set = TocRuleSet::builtin();
        set.rules.push(TocRule {
            name: "bad".into(),
            rule: "(unclosed".into(),
            is_volume: false,
            is_exclude: false,
            enable: true,
            example: None,
            serial_number: 99,
        });
        let (chapters, _) = detect(lines("第1章 甲\n正文。\n"), &set);
        assert_eq!(chapters.len(), 1);
    }
}
