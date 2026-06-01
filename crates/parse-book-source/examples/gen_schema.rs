//! 从 Rust 类型生成 v2 书源的 JSON Schema(尊重 serde 属性,杜绝手抄漂移)。
//!
//! 重新生成(改了配置类型后):
//! ```bash
//! cargo run -p parse-book-source --features schema --example gen_schema \
//!   > crates/parse-book-source/book-source.schema.json
//! ```
//! 防漂移测试 `schema_is_in_sync`(`--features schema`)会校验该文件与类型一致。

fn main() {
    let schema = schemars::schema_for!(parse_book_source::BookSource);
    println!(
        "{}",
        serde_json::to_string_pretty(&schema).expect("serialize schema")
    );
}
