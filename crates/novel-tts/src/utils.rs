use regex::Regex;
use std::sync::LazyLock;

/// 匹配主要句子结束标点符号（句号、感叹号、问号）
static STOP_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[。！？!?]").unwrap());
/// 匹配次要句子分隔标点符号（逗号、分号、冒号）
static SUB_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[,;:，；：]").unwrap());

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct TextSegment {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// 定义文本分割级别枚举
/// Primary: 使用主要标点符号（句号、感叹号、问号）进行分割
/// Secondary: 使用次要标点符号（逗号、分号、冒号）进行分割
#[derive(Clone, Copy, PartialEq, Eq)]
enum SplitLevel {
    Primary,   // 使用 STOP_REGEX
    Secondary, // 使用 SUB_REGEX
}

/// 预处理文本，将长文本拆分为适合TTS处理的短句
///
/// # 参数
/// * `text` - 需要处理的原始文本
/// * `limit` - 每个分段的最大字节数限制
/// * `level` - 分割级别（主要或次要）
/// * `byte_offset` - 当前处理文本相对于原始文本的字节偏移量
///
/// # 返回值
/// 返回一个元组向量，每个元组包含：
/// * 处理后的文本片段（已去除首尾空白）
/// * 该片段在原始文本中的起始字节位置
/// * 该片段在原始文本中的结束字节位置
///
/// # 处理逻辑
/// 1. 按行遍历文本
/// 2. 对于空行直接跳过
/// 3. 对于长度未超限的行直接保留
/// 4. 对于超限的行使用标点符号进行分割
/// 5. 如果分割后的片段仍超限，则使用更细粒度的标点符号继续分割
fn preprocess_text_recursive(
    text: &str,
    limit: usize,
    level: SplitLevel,
    mut byte_offset: usize, // 当前 text 在原始字符串中的起始字节偏移
) -> Vec<TextSegment> {
    // 根据分割级别选择相应的正则表达式
    let regex = match level {
        SplitLevel::Primary => &*STOP_REGEX,
        SplitLevel::Secondary => &*SUB_REGEX,
    };

    let mut result: Vec<TextSegment> = Vec::new();

    // 逐行处理文本
    for line in text.lines() {
        // 计算当前行的字节长度和结束位置
        let line_byte_len = line.len();
        let line_end_offset = byte_offset + line_byte_len;

        // 跳过空行，更新字节偏移量（+1 表示换行符）
        if line.trim().is_empty() {
            byte_offset = line_end_offset + 1; // +1 for '\n'
            continue;
        }

        // 如果整行长度未超过限制，直接添加到结果中
        if line_byte_len <= limit {
            // 整行加入，不拆分
            let cleaned = line.trim().to_string();
            if !cleaned.is_empty() {
                result.push(TextSegment {
                    text: cleaned,
                    start: byte_offset,
                    end: line_end_offset,
                });
            }
            byte_offset = line_end_offset + 1;
            continue;
        }

        // 需要拆分长行
        let mut last_split = 0;

        // 使用正则表达式查找所有匹配的标点符号位置
        for mat in regex.find_iter(line) {
            let end = mat.end();
            // 提取从上次分割点到当前标点符号的位置的文本段
            let segment = &line[last_split..end];
            let seg_len = segment.len();

            // 如果段落长度超限且当前是主要分割级别，则递归使用次要分割级别进行细分
            if seg_len > limit && level == SplitLevel::Primary {
                // 递归用次级分隔符拆分这个 segment
                let sub_offset = byte_offset + last_split;
                let mut sub_parts =
                    preprocess_text_recursive(segment, limit, SplitLevel::Secondary, sub_offset);
                result.append(&mut sub_parts);
            } else {
                // 否则直接添加到结果中
                let cleaned = segment.trim().to_string();
                if !cleaned.is_empty() {
                    result.push(TextSegment {
                        text: cleaned,
                        start: byte_offset + last_split,
                        end: byte_offset + end,
                    });
                }
            }
            // 更新上次分割点
            last_split = end;
        }

        // 处理末尾剩余部分（最后一个标点符号到行尾的部分）
        if last_split < line_byte_len {
            let tail = &line[last_split..];
            // 如果尾部片段仍超限且当前是主要分割级别，则递归使用次要分割级别进行细分
            if tail.len() > limit && level == SplitLevel::Primary {
                let sub_offset = byte_offset + last_split;
                let mut sub_parts =
                    preprocess_text_recursive(tail, limit, SplitLevel::Secondary, sub_offset);
                result.append(&mut sub_parts);
            } else {
                // 否则直接添加到结果中
                let cleaned = tail.trim().to_string();
                if !cleaned.is_empty() {
                    result.push(TextSegment {
                        text: cleaned,
                        start: byte_offset + last_split,
                        end: byte_offset + line_byte_len,
                    });
                }
            }
        }

        // 更新字节偏移量，处理下一行（+1 表示换行符）
        byte_offset = line_end_offset + 1; // +1 for '\n'
    }

    result
}

/// 公开接口函数，用于预处理文本以便进行TTS处理
///
/// # 参数
/// * `text` - 需要处理的原始文本
/// * `limit` - 每个分段的最大字节数限制
///
/// # 返回值
/// 返回处理后的文本片段向量，每个元素包含文本内容及其在原始文本中的字节位置范围
pub fn preprocess_text(text: &str, limit: usize) -> Vec<TextSegment> {
    preprocess_text_recursive(text, limit, SplitLevel::Primary, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_text() {
        let text = "第005章 李小曼
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
        let result = preprocess_text(text, 200);
        for TextSegment {
            text: segment,
            start,
            end,
        } in result
        {
            println!("Segment: '{}', Start: {}, End: {}", segment, start, end);
            assert!(&text[start..end].contains(segment.trim()));
        }
    }

    #[test]
    fn test_empty_and_newline() {
        let text = "第一行。\n\n第二行！";
        let result = preprocess_text(text, 10);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "第一行。");
        assert_eq!(result[1].text, "第二行！");
    }
}
