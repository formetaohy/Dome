use crate::span::{Diagnostic, Sources, Span};
use crate::token::{Keyword, Punctuation, Token, TokenKind};

pub fn tokenize(sources: &Sources, source: u32) -> Result<Vec<Token>, Diagnostic> {
    Lexer {
        sources,
        source,
        chars: sources.source(source).text().chars().collect(),
        at: 0,
    }
    .tokens()
}

struct Lexer<'a> {
    sources: &'a Sources,
    source: u32,
    chars: Vec<char>,
    at: usize,
}

impl Lexer<'_> {
    fn tokens(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        loop {
            self.space();
            let start = self.at;
            let Some(held) = self.peek() else {
                tokens.push(Token {
                    kind: TokenKind::End,
                    span: Span::new(self.source, start, start),
                });
                return Ok(tokens);
            };
            let kind = if held.is_ascii_alphabetic() || held == '_' {
                let word = self.word();
                match Keyword::named(&word) {
                    Some(keyword) => TokenKind::Keyword(keyword),
                    None => TokenKind::Identifier(word),
                }
            } else if held.is_ascii_digit() {
                self.number()?
            } else if held == '"' {
                self.text()?
            } else {
                self.punctuation()?
            };
            tokens.push(Token {
                kind,
                span: Span::new(self.source, start, self.at),
            });
        }
    }

    fn space(&mut self) {
        loop {
            match self.peek() {
                Some(held) if held.is_whitespace() => self.at += 1,
                Some('/') if self.look(1) == Some('/') => {
                    while let Some(held) = self.peek() {
                        self.at += 1;
                        if held == '\n' {
                            break;
                        }
                    }
                }
                _ => return,
            }
        }
    }

    fn word(&mut self) -> String {
        let start = self.at;
        while let Some(held) = self.peek() {
            if held.is_ascii_alphanumeric() || held == '_' {
                self.at += 1;
            } else {
                break;
            }
        }
        self.chars[start..self.at].iter().collect()
    }

    fn number(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.at;
        while self.peek().is_some_and(|held| held.is_ascii_digit()) {
            self.at += 1;
        }
        let mut float = false;
        if self.peek() == Some('.') && self.look(1).is_some_and(|held| held.is_ascii_digit()) {
            float = true;
            self.at += 1;
            while self.peek().is_some_and(|held| held.is_ascii_digit()) {
                self.at += 1;
            }
        }
        let exponent = matches!(self.peek(), Some('e' | 'E'))
            && (self.look(1).is_some_and(|held| held.is_ascii_digit())
                || (matches!(self.look(1), Some('+' | '-'))
                    && self.look(2).is_some_and(|held| held.is_ascii_digit())));
        if exponent {
            float = true;
            self.at += 1;
            if matches!(self.peek(), Some('+' | '-')) {
                self.at += 1;
            }
            while self.peek().is_some_and(|held| held.is_ascii_digit()) {
                self.at += 1;
            }
        }
        let text: String = self.chars[start..self.at].iter().collect();
        let span = Span::new(self.source, start, self.at);
        if float {
            let value = text.parse::<f64>().map_err(|_| {
                self.sources
                    .diagnostic(span, format!("{text} is no number"))
            })?;
            Ok(TokenKind::Float(value))
        } else {
            let value = text.parse::<i64>().map_err(|_| {
                self.sources
                    .diagnostic(span, format!("{text} is no whole number"))
            })?;
            Ok(TokenKind::Int(value))
        }
    }

    fn text(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.at;
        self.at += 1;
        let mut text = String::new();
        loop {
            let Some(held) = self.peek() else {
                return Err(self.sources.diagnostic(
                    Span::new(self.source, start, self.at),
                    "the text ends with its source",
                ));
            };
            self.at += 1;
            match held {
                '"' => return Ok(TokenKind::Text(text)),
                '\\' => {
                    let Some(escape) = self.peek() else {
                        return Err(self.sources.diagnostic(
                            Span::new(self.source, start, self.at),
                            "the text ends with its source",
                        ));
                    };
                    self.at += 1;
                    text.push(match escape {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '0' => '\0',
                        '\\' => '\\',
                        '"' => '"',
                        other => {
                            return Err(self.sources.diagnostic(
                                Span::new(self.source, self.at - 1, self.at),
                                format!("the text escapes no {other}"),
                            ));
                        }
                    });
                }
                '\n' => {
                    return Err(self.sources.diagnostic(
                        Span::new(self.source, start, self.at),
                        "the text ends its line",
                    ));
                }
                other => text.push(other),
            }
        }
    }

    fn punctuation(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.at;
        let held = self.peek().expect("a punctuation holds a sign");
        let Some(single) = Punctuation::from_char(held) else {
            return Err(self.sources.diagnostic(
                Span::new(self.source, start, start + 1),
                format!("the source holds no sign {held}"),
            ));
        };
        self.at += 1;
        let paired = self.peek().and_then(|next| single.paired(next));
        let held = match paired {
            Some(paired) => {
                self.at += 1;
                paired
            }
            None => single,
        };
        Ok(TokenKind::Symbol(held))
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn look(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }
}
