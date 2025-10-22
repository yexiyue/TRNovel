use crate::{
    TTSConfig,
    components::Loading,
    hooks::{UseScrollbar, UseThemeConfig},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Flex, Margin},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_kit::prelude::*;
use std::time::Duration;

#[derive(Default, Props)]
pub struct ReadContentProps {
    pub content: String,
    pub is_scroll: bool,
    pub is_loading: bool,
    pub width: u16,
    pub height: u16,
    pub on_prev: Handler<'static, bool>,
    pub on_next: Handler<'static, ()>,
    pub chapter_name: String,
    pub chapter_percent: f64,
    pub line_percent: Option<State<f64>>,
}

#[component]
pub fn ReadContent(
    props: &mut ReadContentProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();
    let mut is_listening = hooks.use_state(|| false);
    let mut highlight_text = hooks.use_state(String::default);
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let novel_tts = *hooks.use_context::<State<Option<novel_tts::NovelTTS>>>();
    let mut chapter_tts = hooks.use_state(|| None::<novel_tts::ChapterTTS>);
    let mut player = hooks.use_state(|| None::<novel_tts::Player>);

    hooks.use_effect(
        move || {
            if let Some(player) = player.write().take() {
                player.sink.stop();
            }
            if let Some(chapter_tts) = chapter_tts.write().take() {
                chapter_tts.cancel();
            }
            is_listening.set(false);
        },
        props.content.clone(),
    );

    hooks.use_effect(
        || {
            if let Some(player) = player.write().as_mut() {
                player.set_speed(tts_config.read().speed);
                player.set_volume(tts_config.read().volume);
            }
        },
        format!("{}-{}", tts_config.read().speed, tts_config.read().volume),
    );

    hooks.use_async_effect(
        {
            let content = props.content.clone();
            async move {
                if let Some(tts) = novel_tts.read().as_ref()
                    && tts_config.read().auto_play
                    && player.read().is_none()
                    && chapter_tts.read().is_none()
                {
                    let mut chapter = tts.chapter_tts(&content);
                    let (queue_output, mut receiver) =
                        chapter.stream(tts_config.read().voice.into(), |e| {
                            eprintln!("{e:?}");
                        });

                    let texts = chapter.texts.clone();
                    tokio::spawn(async move {
                        while let Some(index) = receiver.recv().await {
                            if let Some(index) = index {
                                highlight_text.set(texts[index].clone());
                            } else {
                                // println!("播放完成");
                            }
                        }
                    });
                    let p = tts.player(queue_output);
                    p.set_speed(tts_config.read().speed);
                    p.set_volume(tts_config.read().volume);
                    is_listening.set(true);
                    player.set(Some(p));
                    chapter_tts.set(Some(chapter));
                }
            }
        },
        (
            props.content.clone(),
            novel_tts.read().is_some(),
            // tts_config.read().voice, (暂时不支持立即切换声音)
        ),
    );

    let paragraph = hooks.use_memo(
        || {
            if !highlight_text.read().is_empty() && is_listening.get() {
                Paragraph::new(highlight(
                    &props.content,
                    &highlight_text.read(),
                    (props.width as usize).saturating_sub(2),
                    Style::from(theme.colors.success_color),
                ))
            } else {
                Paragraph::new(textwrap::fill(
                    &props.content,
                    (props.width as usize).saturating_sub(2),
                ))
            }
        },
        (
            is_listening.get(),
            highlight_text.read().clone(),
            &props.content.clone(),
            props.width,
        ),
    );

    let line_percent = hooks.use_state(|| 0.0);
    let mut line_percent = props.line_percent.unwrap_or(line_percent);

    let is_scroll = props.is_scroll;
    let line_count = paragraph
        .line_count(props.width.saturating_sub(2))
        .saturating_sub(props.height as usize - 1);

    let mut current_line = hooks.use_memo(
        || (line_percent.get() * line_count as f64 * 1000.0).round() as usize / 1000,
        format!("{}-{}", line_count, line_percent.get()),
    );
    let mut current_time = hooks.use_state(String::default);

    hooks.use_future(async move {
        current_time.set(chrono::Local::now().format("%H:%M").to_string());
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    hooks.use_scrollbar(line_count, Some(current_line));

    let mut on_prev = props.on_prev.take();
    let mut on_next = props.on_next.take();

    let props_content = props.content.clone();
    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && is_scroll
        {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if current_line > 0 {
                        current_line -= 1;
                        line_percent.set(current_line as f64 / line_count as f64);
                    } else {
                        on_prev(true);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if current_line < line_count {
                        current_line += 1;
                        line_percent.set(current_line as f64 / line_count as f64);
                    } else {
                        on_next(());
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    on_prev(false);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    on_next(());
                }
                KeyCode::PageUp => {
                    current_line = current_line.saturating_sub(5);
                    line_percent.set(current_line as f64 / line_count as f64);
                }
                KeyCode::PageDown => {
                    current_line = (current_line + 5).min(line_count);
                    line_percent.set(current_line as f64 / line_count as f64);
                }
                KeyCode::Home => {
                    line_percent.set(0.0);
                }
                KeyCode::End => {
                    line_percent.set(1.0);
                }
                KeyCode::Char('+') => {
                    tts_config.write().increase_volume();
                }
                KeyCode::Char('-') => {
                    tts_config.write().decrease_volume();
                }
                KeyCode::Char('p') => {
                    if let Some(player) = player.read().as_ref() {
                        if is_listening.get() {
                            player.pause();
                            is_listening.set(false);
                        } else {
                            player.play();
                            is_listening.set(true);
                        }
                    } else if let Some(tts) = novel_tts.read().as_ref() {
                        let mut chapter = tts.chapter_tts(&props_content);
                        let (queue_output, mut receiver) =
                            chapter.stream(tts_config.read().voice.into(), |e| {
                                eprintln!("{e:?}");
                            });

                        let texts = chapter.texts.clone();
                        tokio::spawn(async move {
                            while let Some(index) = receiver.recv().await {
                                if let Some(index) = index {
                                    highlight_text.set(texts[index].clone());
                                } else {
                                    // println!("播放完成");
                                }
                            }
                        });
                        let p = tts.player(queue_output);
                        p.set_speed(tts_config.read().speed);
                        p.set_volume(tts_config.read().volume);
                        is_listening.set(true);
                        player.set(Some(p));
                        chapter_tts.set(Some(chapter));
                    }
                }
                _ => {}
            }
        }
    });

    element!(Border(
        border_style: theme.basic.border,
        top_title: Line::from(props.chapter_name.to_string()).style(theme.novel.chapter).centered(),
        bottom_title: (if is_listening.get(){
            Line::from(
                format!(
                    "播放中: 播放速度{} / 音量{}",
                    tts_config.read().speed,
                    tts_config.read().volume,
                )
            )
            .style(theme.novel.page)
        }else{
            Line::from("按 p 播放/暂停").style(theme.novel.page)
        }).style(theme.novel.page).centered(),
    ){
        #(if props.is_loading {
            element!(Loading(tip:"加载内容中...")).into_any()
        }else{
            element!(Text(text: paragraph, scroll: (current_line as u16,0))).into_any()
        })
        View(
            flex_direction: Direction::Horizontal,
            justify_content: Flex::SpaceBetween,
            height: Constraint::Length(1),
            margin: Margin::new(1,0),
        ){
            $Line::from(format!("{current_line}/{line_count} 行")).style(theme.novel.page)
            $Line::from(format!("{:.2}% {}",props.chapter_percent, current_time.read().clone())).style(theme.novel.progress).right_aligned()
        }
    })
}

pub fn highlight(
    text: &str,
    query: &str,
    width: usize,
    highlight_style: Style,
) -> Vec<Line<'static>> {
    let pattern: String = regex::escape(query);

    let regex = regex::Regex::new(&pattern).unwrap();
    let res = regex.find(text);
    if let Some(mat) = res {
        let marked = format!(
            "{}<b>{}</b>{}",
            &text[..mat.start()],
            mat.as_str(),
            &text[mat.end()..]
        );
        let highlighted = textwrap::fill(&marked, width);
        // let re_mark = Regex::new(r"(?ms)<b>(.*?)</b>").unwrap();
        // let mat = re_mark.find(&highlighted).unwrap();
        highlight_text(&highlighted, highlight_style)
    } else {
        let texts = textwrap::fill(text, width);
        texts
            .lines()
            .map(|line| Line::from(line.to_string()))
            .collect::<Vec<_>>()
    }
}

fn highlight_text(text: &str, highlight_style: Style) -> Vec<Line<'static>> {
    let mut lines = vec![];
    let mut matched = 0;

    for line in text.lines() {
        let mut spans = vec![];
        let mut highlight = line.to_string();
        if let Some((start, rest)) = line.split_once("<b>") {
            matched += 1;
            spans.push(Span::from(start.to_string()));
            highlight = rest.to_string();
        }

        if matched > 0 {
            if let Some((highlight, end)) = &highlight.split_once("</b>") {
                matched -= 1;
                spans.push(Span::from(highlight.to_string()).style(highlight_style));
                spans.push(Span::from(end.to_string()));
            } else {
                spans.push(Span::from(highlight).style(highlight_style));
            }
        } else {
            spans.push(Span::from(line.to_string()));
        }
        lines.push(Line::from(spans));
    }

    lines
}
// fn highlight_text(
//     text: String,
//     start: usize,
//     end: usize,
//     highlight_style: Style,
// ) -> Vec<Line<'static>> {
//     let mut last_index = 0;
//     let mut lines = vec![];

//     for line in text.lines() {
//         let count: usize = line.chars().map(|c| c.len_utf8()).sum();
//         let new_index = last_index + count;
//         let mut spans = vec![];

//         // 完全在高亮范围之前
//         if new_index < start {
//             spans.push(Span::from(line.to_string()));
//         }
//         // 完全在高亮范围内
//         else if last_index >= start && new_index <= end {
//             spans.push(Span::styled(line.to_string(), highlight_style));
//         }
//         // 完全在高亮范围之后
//         else if last_index >= end {
//             spans.push(Span::from(line.to_string()));
//         }
//         // 高亮跨越多行或者部分在当前行
//         else {
//             // 确保我们在字符边界上进行切片
//             let line_chars: Vec<char> = line.chars().collect();
//             let line_len = line_chars.len();

//             // 计算在当前行内的高亮区域（基于字符索引）
//             let char_start = line_chars
//                 .iter()
//                 .scan(0, |acc, c| {
//                     let start = *acc;
//                     *acc += c.len_utf8();
//                     Some(start)
//                 })
//                 .position(|pos| last_index + pos >= start)
//                 .unwrap_or(0);

//             let char_end = line_chars
//                 .iter()
//                 .scan(0, |acc, c| {
//                     let start = *acc;
//                     *acc += c.len_utf8();
//                     Some(start)
//                 })
//                 .position(|pos| last_index + pos >= end)
//                 .unwrap_or(line_len);

//             // 构建span片段
//             if char_start > 0 {
//                 spans.push(Span::from(
//                     line_chars[..char_start].iter().collect::<String>(),
//                 ));
//             }

//             spans.push(Span::styled(
//                 line_chars[char_start..char_end].iter().collect::<String>(),
//                 highlight_style,
//             ));

//             if char_end < line_len {
//                 spans.push(Span::from(
//                     line_chars[char_end..].iter().collect::<String>(),
//                 ));
//             }
//         }

//         lines.push(Line::from(spans));
//         last_index = new_index + 1;
//     }

//     lines
// }

#[cfg(test)]
mod tests {
    use ratatui::style::Stylize;

    use super::*;

    #[test]
    fn test_regex() {
        let text="第005章 李小曼
　　母校和以前相比并没有太大的变化，变的只是来了又去的人，以四载青春刻印一段难忘的记忆。
　　绿荫下、草地旁，有些学弟学妹在静静地看书，非常和谐与宁静，叶凡等人感觉像是回到了过去，远离了这三年来所经历的浮躁与喧嚣。
　　毕业后，众人为了生活与理想而忙碌，不少人远离了这座城市，除却叶凡等有限几人外，其他人几乎都是第一次重返母校。
　　不远处的小湖微波轻漾，风景依旧，还清晰地记得当年那些或忧郁颓废、或神采飞扬的身影在湖畔抱着吉他弹唱校园民谣的情景。
　　纵然多年过去后，每当旋律响起的时候，总会让人想起那无虑的纯真年代，那淡淡的忧伤让人伤感与甜蜜，很容易打动人的心灵。
　　岁月的沉淀，总会留下些许酸酸楚楚的味道。
　　只是不知道当年那些人如今是否还能抱起吉他弹唱，毕业后很难再寻到他们的去向。
　　“我隐约间听朋友说过，当年那个忧郁的吉他高手在另一座城市的一家酒吧驻唱，几年下来很是沧桑。”
　　“还记得当年校乐队那位多才多艺的长腿妹妹吗，非常漂亮而又清纯的主唱，据说如今在一家夜总会陪酒。”
　　众人只能发出一声叹息。
　　毕业后，很多人都遭遇了理想与实现的冲击。有时候生活真的很无奈，让人倍感挫折与迷茫。
　　短暂沉默后，众人继续向前走去。
　　这时，林佳来到了叶凡的身边。
　　她身穿一条蓝白相间的雪纺连衣裙，裙下摆到大腿处，将两条修长得美腿映衬得更加白皙动人。她扎了一条黑色的腰带，令腰肢更显柔美，长发披散在丰挺的胸前，身形曲线动人。
　　姣好的容颜，雪白的肌肤，具有异样风情的丹凤眼微微向上斜飞，林佳整个人有着一股特别的气质。
　　“明明有车，昨天为什么没有对我说？”
　　“我哪里有机会说。”
　　“今天不邀请我坐你的车走吗？”
　　“非常乐意，在这里我郑重邀请林佳小姐。”
　　说到这里两人都笑了。
　　林佳很突兀的点到了昨天的事情，但又轻飘飘的绕了过去，并没有因为昨天的事而多说些什么，更未因此而刻意放低姿态来拉近关系。
　　说完这些，她便笑着转身离去了。林佳是一个聪明的女子，她知道太过刻意反而不好，那样只会显得虚假，远不如直接与自然一些。
　　这种微妙的变化自然也发生在了其他一些同学的身上。
　　离开母校时已近中午，众人来到美食一条街，登临食府楼。
　　王子文私下请叶凡坐到他们那个桌位去，叶凡只是笑着过去敬了几杯酒，依然与昨天那些人坐在了一起。
　　“叶凡，昨天我醉言醉语，你不要介意。我敬你一杯，先干为敬……”那名说自己未婚妻是某银行高管的侄女的男同学，昨天还对叶凡一副说教的样子，今天却以低姿态极力解释昨天的事情。
　　而那名说自己丈夫已经升职为公司副总的女同学，也一改昨天的姿态，对叶凡客客气气。
　　“来来来，大家同举杯。”
　　……
　　相比昨天，今天叶凡他们这个桌位显得很热闹，众人不断碰杯，不时有其他桌位的人过来敬酒。而叶凡自然推脱不过，连连与人碰杯，更是与王子文那个桌位过来的人逐个喝了一杯。
　　刘云志很淡定，尽管昨天他很尴尬，但今日却古井无波，看不出什么异样的神色，像是什么也没有发生过。
　　“诸位，昨天晚上我接到一个电话，来自大洋彼岸……”
　　说话的是周毅，一个很儒雅的青年，传言家里背景深厚，在同学间已经不是什么秘密。昨天，王子文在海上明月城外专门等候相迎的人便是他。
　　所有人都停了下来，望向周毅，无论是上学时还是现在，他都表现得很随和，从来未让人感觉过倨傲。
　　周毅说了一个消息，在大洋彼岸留学的三位同学已经动身回国，顿时让在场的同学一阵热议。
　　……
　　“毕业后，我们天各一方，每个人都有自己不同的生活轨迹，能够相聚在一起非常不容易。再相见时，或许我们都已经为人父、为人母，到那时也不知道要过去多少年了。三个留学在外的同学要回国了，我有一个提议，稍微延长这次聚会……”
　　……
　　叶凡驱车回到家中，泡上一杯清淡的绿茶，静静地看着窗外的梧桐树，他想起了一些往事。
　　那错过的人，那离去的脚步，那渐行渐远的路，就像是眼前的梧桐叶轻轻地飘落。
　　李小曼，这个名字已经淡出叶凡的记忆很长时间了。
　　大学毕业时李小曼前往大洋彼岸留学，最开始的几个月两人间联系还很密切，但随着时间的推移，往来的电子邮件与电话渐渐变少，最终彻底中断了联系。
　　与其说隔海相望，不如说隔海相忘。一段并不被朋友所看好的爱情，如预料那般走到了终点。
　　今天从周毅口中得知李小曼即将回国，叶凡初听到这个名字时甚至有些陌生的感觉，蓦然回首，已经过去两年多了。
　　……
　　聚会的时间被延长，将去游览泰山，一切花费全部由王子文与周毅等人出，对于常人来说这或许是一笔不菲的开销，但是对于他们来说这并不算什么。
　　三天后，叶凡在泰山脚下再次见到那个熟悉的身影。三年过去了，李小曼依然婀娜挺秀，并没有太大的变化。
　　她身高能有一百七十公分，戴着一副太阳镜，乌黑的长发随风飘舞，站在那里亭亭玉立。她的穿着很简单随意与清凉，下身是一条到膝盖上方的短裤，美腿白皙，修长动人，而上身则是一件印有卡通图案的体恤。
　　李小曼无疑非常美丽，肌肤雪白细嫩，眼睛很大，睫毛很长，显得很有灵气，整个人不张扬但却很自信。
　　她从容自若的与周围的同学交谈，明显成为了一个中心人物，但又可以让人感觉到亲切。
　　在李小曼身边有一个身材高大的青年，据介绍是她的美国同学，相对于东方人面孔的柔润平顺来说，他具有一张典型的西方面孔，很有立体感，鼻梁高挺，碧蓝色的眼睛微微凹陷，金发有些卷曲，以西方人的审美观来说很英俊。
　　“你们好，我是凯德，对泰山……向往，终于可以……看到。”这个名为凯德的美国青年虽然话语不是很流利，但是足以能够表达清楚话意。
　　而前方另外两位留学回国的同学也早已被热情的围住，正在被询问在大洋彼岸的生活与学习情况。
　　时隔三年，叶凡再次见到李小曼，有种空间更迭，时光流转的感觉。
　　两人都波澜不惊，礼貌性的相互问候，没有久别重逢后的喜悦，有的只是平淡如水，甚至有些云淡风轻的味道。
　　没有过多的话语，轻轻擦肩而过，有些事情无需多说，无言就是一种结果。";

        let raw = "非常和谐与宁静，叶凡等人感觉像是回到了过去，";

        println!(
            "-------------------{:#?}",
            highlight(text, raw, 30, Style::default().red())
        );
    }
}
