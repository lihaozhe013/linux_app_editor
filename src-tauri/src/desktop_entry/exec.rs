//! Exec key handling following the quoting rules of the Desktop Entry
//! Specification. The Exec value is never treated as a shell command.
//!
//! Field codes (`%f %F %u %U %i %c %k`) found in argument text are assumed
//! intentional and passed through verbatim. A stray `%` that is not part of a
//! valid field code is escaped as `%%` on write and decoded back on read, so
//! `parse(build(spec)) == spec` holds.

use std::fmt;

use serde::{Deserialize, Serialize};

const FIELD_CODE_CHARS: [char; 7] = ['f', 'F', 'u', 'U', 'i', 'c', 'k'];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecSpec {
    pub executable: String,
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecParseError {
    Empty,
    UnterminatedQuote,
}

impl fmt::Display for ExecParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecParseError::Empty => write!(f, "Exec value is empty"),
            ExecParseError::UnterminatedQuote => {
                write!(f, "unterminated double quote in Exec value")
            }
        }
    }
}

fn needs_quoting(s: &str) -> bool {
    s.is_empty()
        || s.chars().any(|c| {
            matches!(
                c,
                ' ' | '\t'
                    | '\n'
                    | '\r'
                    | '"'
                    | '\''
                    | '\\'
                    | '`'
                    | '$'
                    | '>'
                    | '<'
                    | '~'
                    | '|'
                    | '&'
                    | ';'
                    | '*'
                    | '?'
                    | '#'
                    | '('
                    | ')'
                    | '['
                    | ']'
            )
        })
}

fn quote_token(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if matches!(c, '\\' | '`' | '$' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Each literal `%` that is not part of a valid field code becomes `%%`.
fn encode_percents(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            match chars.get(i + 1).copied() {
                Some(c) if FIELD_CODE_CHARS.contains(&c) => {
                    out.push('%');
                    out.push(c);
                    i += 2;
                }
                _ => {
                    out.push_str("%%");
                    i += 1;
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// `%%` becomes `%`; field codes stay as-is; a stray `%` is kept as-is
/// (validation reports it, the value is not mangled).
fn decode_percents(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            match chars.get(i + 1).copied() {
                Some('%') => {
                    out.push('%');
                    i += 2;
                }
                Some(c) if FIELD_CODE_CHARS.contains(&c) => {
                    out.push('%');
                    out.push(c);
                    i += 2;
                }
                _ => {
                    out.push('%');
                    i += 1;
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn encode_token(s: &str) -> String {
    let escaped = encode_percents(s);
    if needs_quoting(&escaped) {
        quote_token(&escaped)
    } else {
        escaped
    }
}

pub fn build_exec(spec: &ExecSpec) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(spec.arguments.len() + 1);
    parts.push(encode_token(&spec.executable));
    for arg in &spec.arguments {
        parts.push(encode_token(arg));
    }
    parts.join(" ")
}

pub fn parse_exec(value: &str) -> Result<ExecSpec, ExecParseError> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut token_started = false;
    let mut in_quotes = false;
    let mut chars = value.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if !in_quotes => {
                in_quotes = true;
                token_started = true;
            }
            '"' if in_quotes => {
                in_quotes = false;
            }
            '\\' if in_quotes => {
                token_started = true;
                match chars.next() {
                    Some(e @ ('`' | '$' | '"' | '\\')) => current.push(e),
                    Some(other) => {
                        current.push('\\');
                        current.push(other);
                    }
                    None => current.push('\\'),
                }
            }
            ' ' | '\t' if !in_quotes => {
                if token_started {
                    tokens.push(std::mem::take(&mut current));
                    token_started = false;
                }
            }
            _ => {
                token_started = true;
                current.push(c);
            }
        }
    }

    if in_quotes {
        return Err(ExecParseError::UnterminatedQuote);
    }
    if token_started {
        tokens.push(current);
    }
    let Some((first, rest)) = tokens.split_first() else {
        return Err(ExecParseError::Empty);
    };

    Ok(ExecSpec {
        executable: decode_percents(first),
        arguments: rest.iter().map(|t| decode_percents(t)).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(exec: &str, args: &[&str]) -> ExecSpec {
        ExecSpec {
            executable: exec.to_string(),
            arguments: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn build_plain_tokens_stay_unquoted() {
        assert_eq!(
            build_exec(&spec("/usr/bin/foo", &["--verbose"])),
            "/usr/bin/foo --verbose"
        );
    }

    #[test]
    fn build_quotes_spaces() {
        assert_eq!(
            build_exec(&spec("/opt/my app/foo", &["-o out file"])),
            "\"/opt/my app/foo\" \"-o out file\""
        );
    }

    #[test]
    fn build_escapes_quote_dollar_backslash() {
        assert_eq!(
            build_exec(&spec("foo", &[r#"say "hi""#, r"$HOME", r"a\b"])),
            r#"foo "say \"hi\"" "\$HOME" "a\\b""#
        );
    }

    #[test]
    fn build_keeps_field_codes() {
        assert_eq!(build_exec(&spec("foo", &["%f", "%U"])), "foo %f %U");
    }

    #[test]
    fn build_escapes_stray_percent() {
        assert_eq!(build_exec(&spec("foo", &["50%"])), "foo 50%%");
        assert_eq!(build_exec(&spec("foo", &["%%"])), "foo %%%%");
        assert_eq!(build_exec(&spec("foo", &["%z"])), "foo %%z");
    }

    #[test]
    fn build_quotes_empty_argument() {
        assert_eq!(build_exec(&spec("foo", &[""])), r#"foo """#);
    }

    #[test]
    fn parse_simple() {
        assert_eq!(parse_exec("foo --a b").unwrap(), spec("foo", &["--a", "b"]));
    }

    #[test]
    fn parse_quoted() {
        assert_eq!(
            parse_exec(r#""/opt/my app/foo" "-o out file""#).unwrap(),
            spec("/opt/my app/foo", &["-o out file"])
        );
    }

    #[test]
    fn parse_escapes() {
        assert_eq!(
            parse_exec(r#"foo "say \"hi\"" "\$HOME" "a\\b""#).unwrap(),
            spec("foo", &[r#"say "hi""#, "$HOME", r"a\b"])
        );
    }

    #[test]
    fn parse_percent_round_trip() {
        for arg in ["50%", "%%", "%f", "%F %u", "%z", "a%%b%f", "100% done"] {
            assert_eq!(
                parse_exec(&build_exec(&spec("foo", &[arg]))).unwrap(),
                spec("foo", &[arg]),
                "round trip failed for {arg}"
            );
        }
    }

    #[test]
    fn parse_unicode_paths() {
        let s = spec("/home/用户/应用/foo", &["--dir=/tmp/ディレクトリ"]);
        assert_eq!(parse_exec(&build_exec(&s)).unwrap(), s);
    }

    #[test]
    fn parse_empty_and_broken() {
        assert_eq!(parse_exec("   "), Err(ExecParseError::Empty));
        assert_eq!(parse_exec(""), Err(ExecParseError::Empty));
        assert_eq!(
            parse_exec(r#"foo "unclosed"#),
            Err(ExecParseError::UnterminatedQuote)
        );
    }

    #[test]
    fn parse_keeps_field_codes_verbatim() {
        assert_eq!(
            parse_exec("foo %f %U %i").unwrap(),
            spec("foo", &["%f", "%U", "%i"])
        );
    }

    #[test]
    fn general_round_trip() {
        let s = spec(
            "/home/u/My Apps/foo binary",
            &[
                "--profile dev",
                "file with spaces.txt",
                "%f",
                "50%",
                "quote\"inside",
                "$dollar",
            ],
        );
        assert_eq!(parse_exec(&build_exec(&s)).unwrap(), s);
    }
}
