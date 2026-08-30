//! AI 回答的轻量代码高亮：把消息内容切分为文本段与代码段（``` 围栏），
//! 并对代码逐行做关键字/字符串/注释/数字/类型的着色分词。
//! 采用与 JSON 树视图相同的「一行多 token」渲染模型，供 Slint 逐 token 着色。

/// token 着色类别（与 runner.slint 中 CodeToken.kind 对应）
pub const TOKEN_PLAIN: i32 = 0;
pub const TOKEN_KEYWORD: i32 = 1;
pub const TOKEN_STRING: i32 = 2;
pub const TOKEN_COMMENT: i32 = 3;
pub const TOKEN_NUMBER: i32 = 4;
pub const TOKEN_TYPE: i32 = 5;

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Text(String),
    Code { lang: String, code: String },
}

/// 把消息内容按 ``` 围栏切分。流式输出中代码块可能尚未闭合，
/// 此时把剩余部分整体视为代码段。
pub fn split_segments(content: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut text_buf = String::new();
    let mut lines = content.lines().peekable();

    let mut flush_text = |buf: &mut String, out: &mut Vec<Segment>| {
        if !buf.is_empty() {
            out.push(Segment::Text(buf.trim_end_matches('\n').to_string()));
            buf.clear();
        }
    };

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            // 进入代码块
            flush_text(&mut text_buf, &mut segments);
            let lang = rest.trim().to_string();
            let mut code_lines: Vec<String> = Vec::new();
            let mut closed = false;
            for inner in lines.by_ref() {
                if inner.trim_start().starts_with("```") {
                    closed = true;
                    break;
                }
                code_lines.push(inner.to_string());
            }
            let mut code = code_lines.join("\n");
            if !code.is_empty() && closed {
                code.push('\n');
            }
            segments.push(Segment::Code { lang, code });
        } else {
            text_buf.push_str(line);
            text_buf.push('\n');
        }
    }
    flush_text(&mut text_buf, &mut segments);
    segments
}

/// 跨行状态（块注释）
#[derive(Default)]
pub struct ScanState {
    in_block_comment: bool,
}

fn is_comment_hash(lang: &str) -> bool {
    matches!(
        lang,
        "python" | "py" | "bash" | "sh" | "shell" | "yaml" | "yml" | "toml" | "ruby" | "rb"
            | "r" | "perl" | "makefile" | "conf" | "ini" | "dockerfile"
    )
}

fn keyword_set() -> &'static [&'static str] {
    &[
        // 通用控制流
        "if", "else", "elif", "for", "while", "loop", "match", "case", "switch", "do", "then",
        "fi", "done", "break", "continue", "return", "yield", "await", "async", "try", "catch",
        "except", "finally", "throw", "throws", "raise", "with", "as", "in", "of", "is", "not",
        "and", "or", "from", "import", "export", "default", "package", "use", "extern", "pub",
        "public", "private", "protected", "static", "const", "final", "readonly", "let", "var",
        // 声明与类型
        "fn", "func", "def", "function", "class", "struct", "enum", "interface", "trait", "impl",
        "type", "typedef", "namespace", "module", "mod", "crate", "new", "delete", "extends",
        "implements", "super", "this", "self", "Self", "mut", "ref", "move", "dyn", "where",
        "unsafe", "override", "abstract", "virtual", "inline", "const",
        // 类型与字面量
        "true", "false", "null", "nil", "None", "True", "False", "void", "int", "uint", "float",
        "double", "bool", "boolean", "str", "string", "String", "char", "byte", "long", "short",
        "usize", "isize", "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64",
    ]
}

fn is_keyword(word: &str) -> bool {
    keyword_set().contains(&word)
}

/// 对单行代码分词。`state` 携带跨行的块注释状态。
/// 返回 (text, kind) 列表，kind 见 TOKEN_* 常量。
///
/// Tab 会在渲染时显示为缺字形的方框（Slint 文本没有制表位），
/// Go 等语言惯用 Tab 缩进，这里统一展开为 4 个空格；顺带去掉 CRLF 残留的 \r。
pub fn tokenize_line(line: &str, lang: &str, state: &mut ScanState) -> Vec<(String, i32)> {
    let expanded = line.replace('\t', "    ");
    let line = expanded.trim_end_matches('\r');
    let mut tokens: Vec<(String, i32)> = Vec::new();
    let mut push = |tokens: &mut Vec<(String, i32)>, text: String, kind: i32| {
        if text.is_empty() {
            return;
        }
        if let Some(last) = tokens.last_mut() {
            if last.1 == kind {
                last.0.push_str(&text);
                return;
            }
        }
        tokens.push((text, kind));
    };

    let hash_comment = is_comment_hash(lang);
    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0;
    let n = bytes.len();

    while i < n {
        if state.in_block_comment {
            // 找 "*/"
            if i + 1 < n && bytes[i] == '*' && bytes[i + 1] == '/' {
                push(&mut tokens, "*/".into(), TOKEN_COMMENT);
                state.in_block_comment = false;
                i += 2;
            } else {
                let start = i;
                while i < n && !(bytes[i] == '*' && i + 1 < n && bytes[i + 1] == '/') {
                    i += 1;
                }
                push(&mut tokens, bytes[start..i].iter().collect(), TOKEN_COMMENT);
            }
            continue;
        }

        let c = bytes[i];

        // 行注释
        let comment_start = (c == '/' && i + 1 < n && bytes[i + 1] == '/')
            || (c == '-' && i + 1 < n && bytes[i + 1] == '-')
            || (c == '#' && hash_comment);
        if comment_start {
            push(&mut tokens, bytes[i..].iter().collect(), TOKEN_COMMENT);
            break;
        }
        // Rust 属性 #[...] 按普通内容处理（避免把 # 当注释）
        // 块注释开始
        if c == '/' && i + 1 < n && bytes[i + 1] == '*' {
            state.in_block_comment = true;
            push(&mut tokens, "/*".into(), TOKEN_COMMENT);
            i += 2;
            continue;
        }
        // 字符串（行内）
        if c == '"' || c == '\'' || c == '`' {
            let quote = c;
            let start = i;
            i += 1;
            while i < n {
                if bytes[i] == '\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            push(&mut tokens, bytes[start..i.min(n)].iter().collect(), TOKEN_STRING);
            continue;
        }
        // 数字
        if c.is_ascii_digit() {
            let start = i;
            while i < n
                && (bytes[i].is_ascii_alphanumeric()
                    || bytes[i] == '.'
                    || bytes[i] == '_'
                    || ((bytes[i] == '-' || bytes[i] == '+')
                        && (bytes[i - 1] == 'e' || bytes[i - 1] == 'E')))
            {
                i += 1;
            }
            push(&mut tokens, bytes[start..i].iter().collect(), TOKEN_NUMBER);
            continue;
        }
        // 标识符/关键字/类型名
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < n && (bytes[i].is_alphanumeric() || bytes[i] == '_') {
                i += 1;
            }
            let word: String = bytes[start..i].iter().collect();
            let kind = if is_keyword(&word) {
                TOKEN_KEYWORD
            } else if word.chars().next().is_some_and(|ch| ch.is_uppercase()) {
                TOKEN_TYPE
            } else {
                TOKEN_PLAIN
            };
            push(&mut tokens, word, kind);
            continue;
        }
        // 其余字符
        let start = i;
        while i < n {
            let ch = bytes[i];
            if ch.is_alphabetic()
                || ch == '_'
                || ch.is_ascii_digit()
                || ch == '"'
                || ch == '\''
                || ch == '`'
                || (ch == '/' && i + 1 < n && (bytes[i + 1] == '/' || bytes[i + 1] == '*'))
                || (ch == '-' && i + 1 < n && bytes[i + 1] == '-')
                || (ch == '#' && hash_comment)
            {
                break;
            }
            i += 1;
        }
        push(&mut tokens, bytes[start..i].iter().collect(), TOKEN_PLAIN);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_segments_basic() {
        let content = "说明如下：\n\n```rust\nfn main() {}\n```\n完毕";
        let segs = split_segments(content);
        assert_eq!(segs.len(), 3);
        assert!(matches!(&segs[0], Segment::Text(t) if t.contains("说明如下")));
        match &segs[1] {
            Segment::Code { lang, code } => {
                assert_eq!(lang, "rust");
                assert!(code.contains("fn main()"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(&segs[2], Segment::Text(t) if t.contains("完毕")));
    }

    #[test]
    fn test_split_segments_unterminated_fence_streaming() {
        let content = "前文\n```python\nprint(1)";
        let segs = split_segments(content);
        assert_eq!(segs.len(), 2);
        match &segs[1] {
            Segment::Code { lang, code } => {
                assert_eq!(lang, "python");
                assert!(code.contains("print(1)"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_split_segments_no_code() {
        let segs = split_segments("普通回答，没有代码。");
        assert_eq!(segs.len(), 1);
        assert!(matches!(&segs[0], Segment::Text(t) if t.contains("普通回答")));
    }

    #[test]
    fn test_tokenize_rust_line() {
        let mut state = ScanState::default();
        let tokens = tokenize_line("let s = \"hi\"; // 注释", "rust", &mut state);
        let kinds: Vec<i32> = tokens.iter().map(|t| t.1).collect();
        assert!(kinds.contains(&TOKEN_KEYWORD), "{tokens:?}");
        assert!(kinds.contains(&TOKEN_STRING), "{tokens:?}");
        assert!(kinds.contains(&TOKEN_COMMENT), "{tokens:?}");
        assert_eq!(*kinds.last().unwrap(), TOKEN_COMMENT);
    }

    #[test]
    fn test_tokenize_type_and_number() {
        let mut state = ScanState::default();
        let tokens = tokenize_line("let x: Vec<u32> = Vec::new(); // 说明", "rust", &mut state);
        let joined: String = tokens.iter().map(|t| t.0.clone()).collect();
        assert_eq!(joined, "let x: Vec<u32> = Vec::new(); // 说明");
        assert!(tokens.iter().any(|t| t.0 == "Vec" && t.1 == TOKEN_TYPE));
        assert!(tokens.iter().any(|t| t.0 == "u32" && t.1 == TOKEN_KEYWORD));
        assert!(tokens.iter().any(|t| t.1 == TOKEN_COMMENT));

        let mut state = ScanState::default();
        let tokens = tokenize_line("let n = 42;", "rust", &mut state);
        assert!(tokens.iter().any(|t| t.0 == "42" && t.1 == TOKEN_NUMBER), "{tokens:?}");
    }

    #[test]
    fn test_tokenize_expands_tabs_and_strips_cr() {
        let mut state = ScanState::default();
        let tokens = tokenize_line("\tfn main() {\r", "go", &mut state);
        let joined: String = tokens.iter().map(|t| t.0.clone()).collect();
        // Tab 展开为 4 空格、\r 被去掉，不含任何控制字符
        assert_eq!(joined, "    fn main() {");
        assert!(!joined.contains('\t') && !joined.contains('\r'));

        // 纯 Tab 行（连续缩进）
        let mut state = ScanState::default();
        let tokens = tokenize_line("\t\treturn nil", "go", &mut state);
        let joined: String = tokens.iter().map(|t| t.0.clone()).collect();
        assert_eq!(joined, "        return nil");
        assert!(tokens.iter().any(|t| t.0 == "return" && t.1 == TOKEN_KEYWORD));
    }

    #[test]
    fn test_tokenize_block_comment_across_lines() {
        let mut state = ScanState::default();
        let l1 = tokenize_line("/* start", "cpp", &mut state);
        assert!(l1.iter().all(|t| t.1 == TOKEN_COMMENT));
        assert!(state.in_block_comment);
        let l2 = tokenize_line("end */ fn", "cpp", &mut state);
        assert!(!state.in_block_comment);
        assert!(l2.iter().any(|t| t.0 == "fn" && t.1 == TOKEN_KEYWORD));
    }

    #[test]
    fn test_tokenize_hash_comment_only_for_script_langs() {
        let mut state = ScanState::default();
        let py = tokenize_line("# 注释", "python", &mut state);
        assert_eq!(py.len(), 1);
        assert_eq!(py[0].1, TOKEN_COMMENT);

        let mut state = ScanState::default();
        let rust = tokenize_line("#[derive(Debug)]", "rust", &mut state);
        let joined: String = rust.iter().map(|t| t.0.clone()).collect();
        assert_eq!(joined, "#[derive(Debug)]");
        assert!(!rust.iter().any(|t| t.1 == TOKEN_COMMENT));
    }
}
