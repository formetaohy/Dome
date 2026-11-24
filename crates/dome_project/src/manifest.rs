use dome_toml::{Fields, Toml};

pub(crate) const FORMAT: &str = "dome_project";

pub(crate) struct Manifest {
    pub(crate) name: String,
}

impl Manifest {
    pub(crate) fn parse(text: &str, what: &str) -> Result<Self, String> {
        let mut fields = Fields::of(Toml::parse(text, what)?, what)?;
        let format = fields
            .need("format")?
            .string_value(&format!("the format of {what}"))?;
        if format != FORMAT {
            return Err(format!("{what} holds the format {format}"));
        }
        let name = fields
            .need("name")?
            .string_value(&format!("the name of {what}"))?;
        fields.done()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("{what} holds no name"));
        }
        Ok(Self {
            name: String::from(name),
        })
    }

    pub(crate) fn write(&self) -> String {
        Toml::object([
            ("format", Toml::string(FORMAT)),
            ("name", Toml::string(self.name.as_str())),
        ])
        .write()
    }
}
