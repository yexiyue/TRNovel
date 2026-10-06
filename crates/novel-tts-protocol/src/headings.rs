//! Built-in TOC patterns shared with speech block boundary recognition.
//! Constants only: the protocol does not compile regexes or parse books.
pub const MAX_TITLE_CHARS: usize = 35;
pub const NUMBERS: &str = r"[0-9〇零一二两三四五六七八九十百千万壹贰叁肆伍陆柒捌玖拾佰仟]";
pub const CHINESE_CHAPTER: &str = r"^第[0-9〇零一二两三四五六七八九十百千万壹贰叁肆伍陆柒捌玖拾佰仟]{1,12}[章节回话](?:[ 　\t、，,:：．.\-—_~·].*)?$";
pub const ENGLISH_CHAPTER: &str = r"^(?:[Cc]hapter|[Ss]ection|[Pp]art|[Ee]pisode)\s*\d{1,4}";
pub const SPECIAL_CHAPTER: &str =
    r"^(?:楔子|引子|序章|序言|前言|后记|尾声|终章|完本感言|番外|外传|附录|内容简介|作品相关)";
