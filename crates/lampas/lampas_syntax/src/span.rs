use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub source: u32,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(source: u32, start: usize, end: usize) -> Self {
        Self {
            source,
            start: start as u32,
            end: end as u32,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    name: String,
    text: String,
    lines: Vec<u32>,
    length: u32,
}

impl Source {
    fn new(name: String, text: String) -> Self {
        let mut lines = vec![0];
        for (at, held) in text.chars().enumerate() {
            if held == '\n' {
                lines.push(at as u32 + 1);
            }
        }
        let length = text.chars().count() as u32;
        Self {
            name,
            text,
            lines,
            length,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn place(&self, at: u32) -> (u32, u32, String) {
        let line = self
            .lines
            .partition_point(|start| *start <= at)
            .saturating_sub(1);
        let start = self.lines[line] as usize;
        let end = match self.lines.get(line + 1) {
            Some(next) => *next as usize - 1,
            None => self.length as usize,
        };
        let excerpt = self
            .text
            .chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect::<String>()
            .trim_end()
            .to_string();
        (
            line as u32 + 1,
            at.saturating_sub(start as u32) + 1,
            excerpt,
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Sources {
    sources: Vec<Source>,
}

impl Sources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> u32 {
        let id = self.sources.len() as u32;
        self.sources.push(Source::new(name.into(), text.into()));
        id
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn source(&self, id: u32) -> &Source {
        &self.sources[id as usize]
    }

    pub fn diagnostic(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        let source = self.source(span.source);
        let (line, column, excerpt) = source.place(span.start);
        Diagnostic {
            source: source.name.clone(),
            line,
            column,
            message: message.into(),
            excerpt,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    source: String,
    line: u32,
    column: u32,
    message: String,
    excerpt: String,
}

impl Diagnostic {
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn column(&self) -> u32 {
        self.column
    }

    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            out,
            "{}:{}:{}: {}",
            self.source, self.line, self.column, self.message,
        )?;
        writeln!(out, "    {}", self.excerpt)?;
        write!(
            out,
            "    {}^",
            " ".repeat(self.column.saturating_sub(1) as usize),
        )
    }
}

impl std::error::Error for Diagnostic {}
