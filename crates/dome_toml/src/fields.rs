use crate::Toml;

pub struct Fields {
    what: String,
    pairs: Vec<(String, Toml)>,
}

impl Fields {
    pub fn of(value: Toml, what: impl Into<String>) -> Result<Self, String> {
        let what = what.into();
        Ok(Self {
            pairs: value.object_fields(&what)?,
            what,
        })
    }

    pub fn take(&mut self, key: &str) -> Option<Toml> {
        let at = self.pairs.iter().position(|(held, _)| held == key)?;
        Some(self.pairs.remove(at).1)
    }

    pub fn need(&mut self, key: &str) -> Result<Toml, String> {
        self.take(key)
            .ok_or_else(|| format!("{} holds no `{key}`", self.what))
    }

    pub fn done(self) -> Result<(), String> {
        if let Some((key, _)) = self.pairs.first() {
            return Err(format!("{} holds the unknown field `{key}`", self.what));
        }
        Ok(())
    }
}

pub fn floats<const N: usize>(value: Toml, what: &str) -> Result<[f32; N], String> {
    let items = value.array_items(what)?;
    if items.len() != N {
        return Err(format!(
            "{what} holds {} numbers where {N} stand",
            items.len(),
        ));
    }
    let mut items = items.into_iter();
    let mut numbers = [0.0; N];
    for number in &mut numbers {
        *number = items
            .next()
            .expect("a counted array holds every number")
            .f32_value(what)?;
    }
    Ok(numbers)
}
