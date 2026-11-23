use crate::Toml;

pub struct Fields {
    what: String,
    pairs: Vec<(String, Toml)>,
}

impl Fields {
    pub fn of(value: Toml, what: impl Into<String>) -> Self {
        let what = what.into();
        Self {
            pairs: value.object_fields(&what),
            what,
        }
    }

    pub fn take(&mut self, key: &str) -> Option<Toml> {
        let at = self.pairs.iter().position(|(held, _)| held == key)?;
        Some(self.pairs.remove(at).1)
    }

    pub fn need(&mut self, key: &str) -> Toml {
        self.take(key)
            .unwrap_or_else(|| panic!("{} holds no `{key}`", self.what))
    }

    pub fn done(self) {
        if let Some((key, _)) = self.pairs.first() {
            panic!("{} holds the unknown field `{key}`", self.what);
        }
    }
}

pub fn floats<const N: usize>(value: Toml, what: &str) -> [f32; N] {
    let items = value.array_items(what);
    assert_eq!(
        items.len(),
        N,
        "{what} holds {} numbers where {N} stand",
        items.len(),
    );
    let mut items = items.into_iter();
    std::array::from_fn(|_| {
        items
            .next()
            .expect("a counted array holds every number")
            .f32_value(what)
    })
}
