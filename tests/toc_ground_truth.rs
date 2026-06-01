//! 章节解析引擎的真值回归测试。
//!
//! 用 `test-novels/` 下的真实样本验证 detect 引擎的准确率。
//! 这些大文件不随仓库提交，缺失时测试自动跳过（不影响 CI）。

use std::path::PathBuf;
use trnovel::novel::VolumeMarker;
use trnovel::novel::toc_rule::{TocRuleSet, detect};

/// 把文件内容按行切分为 `(整行, 行起始字节偏移)`，模拟逐行读取。
fn read_lines(path: &PathBuf) -> Vec<(String, usize)> {
    let content = std::fs::read_to_string(path).expect("read fixture");
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        out.push((line.to_string(), offset));
        offset += line.len();
    }
    out
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-novels")
        .join(name)
}

fn run(name: &str) -> (Vec<(String, usize)>, Vec<VolumeMarker>) {
    let path = fixture(name);
    let lines = read_lines(&path);
    detect(lines, &TocRuleSet::builtin())
}

#[test]
fn tunshi_xingkong_500_chapters_no_volume() {
    let path = fixture("吞噬星空(1-500章).txt");
    if !path.exists() {
        eprintln!("skip: fixture missing {}", path.display());
        return;
    }
    let (chapters, volumes) = run("吞噬星空(1-500章).txt");
    assert_eq!(chapters.len(), 500, "章节数应为 500");
    assert_eq!(volumes.len(), 0, "应无分卷");
}

#[test]
fn guzhenren_6_volumes_2334_chapters() {
    let path = fixture("《蛊真人》作者：蛊真人.txt");
    if !path.exists() {
        eprintln!("skip: fixture missing {}", path.display());
        return;
    }
    let (chapters, volumes) = run("《蛊真人》作者：蛊真人.txt");
    assert_eq!(volumes.len(), 6, "卷数应为 6");
    assert_eq!(chapters.len(), 2334, "章节(节)数应为 2334");
    // 首卷应从第 0 章开始
    assert_eq!(volumes[0].first_chapter_index, 0);
}
