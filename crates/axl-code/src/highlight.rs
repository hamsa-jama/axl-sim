//! A small tokenizer for C and Rust, enough to colour source in the editor.

use axl_host::toolchain::Lang;

/// What a stretch of source is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Keyword.
    Keyword,
    /// Type name (builtin types and capitalised names).
    Type,
    /// A call to an `axl_` function or an `AXL_` constant.
    Axl,
    /// Number.
    Number,
    /// String or character literal.
    Str,
    /// Comment.
    Comment,
    /// Preprocessor line (`#include`).
    Preproc,
    /// Anything else.
    Plain,
}

/// A byte range of the source and its kind. Ranges tile the whole text in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Start byte.
    pub start: usize,
    /// End byte (exclusive).
    pub end: usize,
    /// Kind.
    pub kind: Kind,
}

const C_KEYWORDS: &[&str] = &[
    "if", "else", "for", "while", "do", "switch", "case", "default", "break", "continue", "return",
    "static", "const", "struct", "enum", "typedef", "sizeof", "inline", "volatile", "extern",
    "goto", "union",
];
const C_TYPES: &[&str] = &[
    "int", "double", "float", "char", "void", "long", "short", "unsigned", "signed", "bool",
    "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t",
    "size_t",
];
const RUST_KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "if", "else", "for", "while", "loop", "match", "return", "use", "mod",
    "pub", "struct", "enum", "impl", "trait", "const", "static", "as", "in", "break", "continue",
    "true", "false", "self", "Self", "crate", "unsafe", "where", "move", "ref",
];
const RUST_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize", "f32", "f64", "bool",
    "char", "str", "String", "Vec", "Option", "Result",
];

/// Split `src` into spans.
pub fn highlight(src: &str, lang: Lang) -> Vec<Span> {
    let b = src.as_bytes();
    let (keywords, types) = match lang {
        Lang::C => (C_KEYWORDS, C_TYPES),
        Lang::Rust => (RUST_KEYWORDS, RUST_TYPES),
    };
    let mut out: Vec<Span> = Vec::new();
    let mut push = |start: usize, end: usize, kind: Kind| {
        if end > start {
            match out.last_mut() {
                Some(l) if l.kind == kind && l.end == start && kind == Kind::Plain => l.end = end,
                _ => out.push(Span { start, end, kind }),
            }
        }
    };
    let mut i = 0;
    let mut line_start = true;
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            push(i, i + 1, Kind::Plain);
            i += 1;
            line_start = true;
            continue;
        }
        if c == b' ' || c == b'\t' || c == b'\r' {
            push(i, i + 1, Kind::Plain);
            i += 1;
            continue;
        }
        let here = i;
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            push(here, i, Kind::Comment);
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(b.len());
            push(here, i, Kind::Comment);
        } else if c == b'#' && lang == Lang::C && line_start {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            push(here, i, Kind::Preproc);
        } else if c == b'"' || (c == b'\'' && lang == Lang::C) {
            i += 1;
            while i < b.len() && b[i] != c && b[i] != b'\n' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(b.len());
            push(here, i, Kind::Str);
        } else if c.is_ascii_digit() {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            push(here, i, Kind::Number);
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let w = &src[here..i];
            let kind = if keywords.contains(&w) {
                Kind::Keyword
            } else if types.contains(&w) {
                Kind::Type
            } else if w.starts_with("axl_") || w.starts_with("AXL_") {
                Kind::Axl
            } else if lang == Lang::Rust && w.starts_with(|c: char| c.is_ascii_uppercase()) {
                Kind::Type
            } else {
                Kind::Plain
            };
            push(here, i, kind);
        } else {
            // Any other character (possibly multi-byte).
            i += src[i..].chars().next().map_or(1, char::len_utf8);
            push(here, i, Kind::Plain);
        }
        line_start = false;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str, lang: Lang) -> Vec<(&str, Kind)> {
        highlight(src, lang)
            .into_iter()
            .filter(|s| s.kind != Kind::Plain)
            .map(|s| (&src[s.start..s.end], s.kind))
            .collect()
    }

    #[test]
    fn c_tokens() {
        let k = kinds(
            "#include \"axl.h\"\nint main(void) { // go\n  axl_motor_power(l, 0.5); return 0; }",
            Lang::C,
        );
        assert_eq!(k[0], ("#include \"axl.h\"", Kind::Preproc));
        assert!(k.contains(&("int", Kind::Type)));
        assert!(k.contains(&("// go", Kind::Comment)));
        assert!(k.contains(&("axl_motor_power", Kind::Axl)));
        assert!(k.contains(&("0.5", Kind::Number)));
        assert!(k.contains(&("return", Kind::Keyword)));
    }

    #[test]
    fn rust_tokens() {
        let k = kinds(
            "let mut x: f64 = 1.5; /* c */ let s = \"a\\\"b\";",
            Lang::Rust,
        );
        assert!(k.contains(&("let", Kind::Keyword)));
        assert!(k.contains(&("f64", Kind::Type)));
        assert!(k.contains(&("/* c */", Kind::Comment)));
        assert!(k.contains(&("\"a\\\"b\"", Kind::Str)));
    }

    #[test]
    fn spans_tile_the_text_even_with_unicode_and_open_comments() {
        for src in ["héllo /* open", "a\n\"unterminated\nb", "", "x = 'c' + é;"] {
            let spans = highlight(src, Lang::C);
            let mut at = 0;
            for s in &spans {
                assert_eq!(s.start, at);
                at = s.end;
            }
            assert_eq!(at, src.len());
        }
    }
}
