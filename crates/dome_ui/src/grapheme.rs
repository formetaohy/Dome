use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Boundary(usize);

impl Boundary {
    pub const fn start() -> Self {
        Self(0)
    }

    pub fn end(text: &str) -> Self {
        Self(text.len())
    }

    pub fn floor(text: &str, byte: usize) -> Self {
        let byte = byte.min(text.len());
        let boundaries = boundaries(text);
        boundaries[boundaries.partition_point(|boundary| boundary.0 <= byte) - 1]
    }

    pub fn ceil(text: &str, byte: usize) -> Self {
        let byte = byte.min(text.len());
        let boundaries = boundaries(text);
        boundaries[boundaries.partition_point(|boundary| boundary.0 < byte)]
    }

    pub fn nearest(text: &str, byte: usize) -> Self {
        let byte = byte.min(text.len());
        let boundaries = boundaries(text);
        let index = boundaries.partition_point(|boundary| boundary.0 < byte);
        let after = boundaries[index];
        if index == 0 {
            return after;
        }
        let before = boundaries[index - 1];
        if byte - before.0 <= after.0 - byte {
            before
        } else {
            after
        }
    }

    pub fn byte(self) -> usize {
        self.0
    }

    pub fn next(self, text: &str) -> Option<Self> {
        text[self.0..]
            .graphemes(true)
            .next()
            .map(|cluster| Self(self.0 + cluster.len()))
    }

    pub fn previous(self, text: &str) -> Option<Self> {
        text[..self.0]
            .graphemes(true)
            .next_back()
            .map(|cluster| Self(self.0 - cluster.len()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    Word,
    Space,
    Symbol,
}

#[derive(Clone, Copy)]
pub(crate) struct Cluster<'a> {
    pub(crate) start: Boundary,
    pub(crate) end: Boundary,
    text: &'a str,
}

impl Cluster<'_> {
    pub(crate) fn class(self) -> Class {
        match self.text.chars().next() {
            Some(character) if character.is_whitespace() => Class::Space,
            Some(character) if character.is_alphanumeric() || character == '_' => Class::Word,
            _ => Class::Symbol,
        }
    }
}

pub(crate) fn boundaries(text: &str) -> Vec<Boundary> {
    offsets(text).map(Boundary).collect()
}

pub(crate) fn clusters(text: &str) -> impl Iterator<Item = Cluster<'_>> + '_ {
    text.grapheme_indices(true)
        .map(|(offset, cluster)| Cluster {
            start: Boundary(offset),
            end: Boundary(offset + cluster.len()),
            text: cluster,
        })
}

fn offsets(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.grapheme_indices(true)
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
}
