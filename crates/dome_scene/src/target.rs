use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Node(String),
    Mesh(String),
    Material(String),
    Image(String),
    Model(String),
    Sound(String),
    Graph(String),
    Environment(String),
}

impl Target {
    pub fn parse(text: &str) -> Option<Self> {
        let (kind, id) = text.split_once('.')?;
        if !valid_id(id) {
            return None;
        }
        let id = String::from(id);
        match kind {
            "node" => Some(Self::Node(id)),
            "mesh" => Some(Self::Mesh(id)),
            "material" => Some(Self::Material(id)),
            "image" => Some(Self::Image(id)),
            "model" => Some(Self::Model(id)),
            "sound" => Some(Self::Sound(id)),
            "graph" => Some(Self::Graph(id)),
            "environment" => Some(Self::Environment(id)),
            _ => None,
        }
    }

    pub fn node(id: impl Into<String>) -> Self {
        Self::Node(id.into())
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Node(_) => "node",
            Self::Mesh(_) => "mesh",
            Self::Material(_) => "material",
            Self::Image(_) => "image",
            Self::Model(_) => "model",
            Self::Sound(_) => "sound",
            Self::Graph(_) => "graph",
            Self::Environment(_) => "environment",
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(id)
            | Self::Mesh(id)
            | Self::Material(id)
            | Self::Image(id)
            | Self::Model(id)
            | Self::Sound(id)
            | Self::Graph(id)
            | Self::Environment(id) => write!(formatter, "{}.{id}", self.kind()),
        }
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}
