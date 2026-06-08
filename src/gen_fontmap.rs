//! `trn gen-fontmap`:字形位图相似度匹配,从「字体反爬」的加密字体生成
//! `{码点: 真字}` 映射表(可直接内联进书源的 `fontMap`)。
//!
//! 原理:加密字体把真字的字形画在私有区(PUA)码点上,但字形「长相」不变。
//! 于是渲染每个 PUA 字形,再和基准中文字体(思源/Noto)逐个常用字比像素,最像的即真字。
//! 详见 `dev-notes/blog/font-anti-scraping-and-fontmap.md`。
//!
//! 纯 Rust、零 C 依赖:woff2 解压(`woff2`)+ 字形轮廓与光栅化(`swash` = skrifa + zeno)。

use anyhow::{Context, Result, anyhow};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Scaler, Source};
use zeno::Format;

/// 基准字体缺省下载源(思源黑体简体 = Noto Sans CJK SC,CFF 轮廓)。
const NOTO_URL: &str = "https://cdn.jsdelivr.net/gh/notofonts/noto-cjk@main/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf";
/// 渲染字号 / 归一化网格边长。
const SIZE: f32 = 64.0;
const DIM: usize = 64;
/// 低置信阈值(top1 余弦相似度低于此,提示人工核对)。
const LOW_CONF: f32 = 0.55;
/// Unicode 私有区范围。
const PUA: std::ops::RangeInclusive<u32> = 0xE000..=0xF8FF;

/// 命令入口:失败打印并退出(与 doctor/import 一致)。
pub async fn run(font: &str, out: &Path, base_font: Option<&Path>) {
    if let Err(e) = try_run(font, out, base_font).await {
        eprintln!("✗ gen-fontmap 失败:{e:#}");
        std::process::exit(1);
    }
}

async fn try_run(font: &str, out: &Path, base_font: Option<&Path>) -> Result<()> {
    // 1. 载入并(如需)解压加密字体。
    let enc_sfnt = to_sfnt(load_font(font).await.context("载入加密字体")?)?;
    let enc = FontRef::from_index(&enc_sfnt, 0).ok_or_else(|| anyhow!("加密字体解析失败"))?;

    // 2. 基准字体:--base-font 或自动下载 Noto。
    let base_bytes = match base_font {
        Some(p) => std::fs::read(p).with_context(|| format!("读取基准字体 {}", p.display()))?,
        None => ensure_noto().await?,
    };
    let base_sfnt = to_sfnt(base_bytes)?;
    let base = FontRef::from_index(&base_sfnt, 0).ok_or_else(|| anyhow!("基准字体解析失败"))?;

    // 3. 加密字体覆盖的 PUA 码点。
    let pua: Vec<char> = PUA
        .filter_map(char::from_u32)
        .filter(|&c| enc.charmap().map(c) != 0)
        .collect();
    if pua.is_empty() {
        return Err(anyhow!(
            "加密字体里没有私有区(PUA)字形,可能不是字体反爬字体"
        ));
    }
    eprintln!("加密字体覆盖 {} 个 PUA 码点", pua.len());

    // 4. 渲染候选:GB2312 一级常用字(正文几乎全在其中,避免匹配到生僻字)。
    let cands = gb2312_level1();
    eprintln!("基准候选字 {} 个,渲染中…", cands.len());
    let mut bctx = ScaleContext::new();
    let mut bscaler = bctx.builder(base).size(SIZE).hint(false).build();
    let cand_vecs: Vec<(char, Vec<f32>)> = cands
        .iter()
        .filter_map(|&c| render_norm(&mut bscaler, base.charmap().map(c)).map(|v| (c, v)))
        .collect();

    // 5. 渲染每个 PUA 字形,匹配最像的候选字。
    let mut ectx = ScaleContext::new();
    let mut escaler = ectx.builder(enc).size(SIZE).hint(false).build();
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut low = Vec::new();
    for &c in &pua {
        let Some(v) = render_norm(&mut escaler, enc.charmap().map(c)) else {
            continue;
        };
        let (best, score) = cand_vecs
            .iter()
            .map(|(cc, cv)| (*cc, dot(&v, cv)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .ok_or_else(|| anyhow!("基准候选为空"))?;
        map.insert(format!("{:04X}", c as u32), best.to_string());
        if score < LOW_CONF {
            low.push((c, best, score));
        }
    }

    // 6. 写出 + 报告。
    std::fs::write(out, serde_json::to_string(&map)?)
        .with_context(|| format!("写出 {}", out.display()))?;
    eprintln!("✓ 已生成 {} 项映射 → {}", map.len(), out.display());
    if !low.is_empty() {
        eprintln!("⚠ {} 个低置信(<{LOW_CONF})映射,建议人工核对:", low.len());
        for (c, best, score) in &low {
            eprintln!("    U+{:04X} → {best}  ({score:.2})", *c as u32);
        }
    }
    Ok(())
}

/// 渲染一个字形到 DIM×DIM 居中灰度网格,并 L2 归一化;空字形返回 None。
fn render_norm(scaler: &mut Scaler<'_>, gid: u16) -> Option<Vec<f32>> {
    let img = Render::new(&[Source::Outline])
        .format(Format::Alpha)
        .render(scaler, gid)?;
    let (w, h) = (img.placement.width as usize, img.placement.height as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let mut g = vec![0f32; DIM * DIM];
    let ox = DIM.saturating_sub(w) / 2;
    let oy = DIM.saturating_sub(h) / 2;
    for y in 0..h.min(DIM) {
        for x in 0..w.min(DIM) {
            g[(oy + y) * DIM + ox + x] = img.data[y * w + x] as f32;
        }
    }
    let norm = g.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm < 1e-6 {
        return None;
    }
    for v in &mut g {
        *v /= norm;
    }
    Some(g)
}

/// 两个等长向量点积(均已 L2 归一化 → 即余弦相似度)。
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// GB2312 一级汉字(3755 个,按拼音序)。用 GBK 解码区位生成。
fn gb2312_level1() -> Vec<char> {
    let mut out = Vec::with_capacity(3755);
    for hi in 0xB0u8..=0xD7 {
        for lo in 0xA1u8..=0xFE {
            let bytes = [hi, lo];
            let (s, _, had_err) = encoding_rs::GBK.decode(&bytes);
            if had_err {
                continue;
            }
            if let Some(c) = s.chars().next()
                && ('\u{4E00}'..='\u{9FFF}').contains(&c)
            {
                out.push(c);
            }
        }
    }
    out
}

/// woff2 → sfnt(ttf/otf);已是 sfnt 则原样返回。
fn to_sfnt(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if woff2::decode::is_woff2(&bytes) {
        woff2::decode::convert_woff2_to_ttf(&mut Cursor::new(bytes))
            .map_err(|e| anyhow!("woff2 解压失败:{e:?}"))
    } else {
        Ok(bytes)
    }
}

/// 从 URL 或本地路径载入字体字节。
async fn load_font(src: &str) -> Result<Vec<u8>> {
    if src.starts_with("http://") || src.starts_with("https://") {
        Ok(reqwest::get(src)
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec())
    } else {
        std::fs::read(src).with_context(|| format!("读取 {src}"))
    }
}

/// 确保基准字体存在(缓存到 `~/.novel/gen-fontmap/`),首次自动下载 Noto。
async fn ensure_noto() -> Result<Vec<u8>> {
    let dir = crate::utils::novel_catch_dir()?.join("gen-fontmap");
    let path: PathBuf = dir.join("NotoSansCJKsc-Regular.otf");
    if path.exists() {
        return Ok(std::fs::read(&path)?);
    }
    eprintln!("未指定 --base-font,自动下载基准字体 Noto Sans CJK SC(~16MB)…");
    let bytes = reqwest::get(NOTO_URL)
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, &bytes)?;
    Ok(bytes)
}
