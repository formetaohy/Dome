use crate::document::Document;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    Repeat,
    MirroredRepeat,
    ClampToEdge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sampler {
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub mip_filter: Filter,
    pub wrap_u: Wrap,
    pub wrap_v: Wrap,
}

pub(crate) fn image_samplers(document: &Document, what: &str) -> Vec<Sampler> {
    let mut samplers = vec![default_sampler(); document.images.len()];
    let mut held = vec![None; document.images.len()];
    for (index, texture) in document.textures.iter().enumerate() {
        let Some(source) = texture.source else {
            continue;
        };
        assert!(
            source < document.images.len(),
            "texture {index} of {what} names the image {source}, which the model does not hold",
        );
        let sampler = read(
            document,
            texture.sampler,
            &format!("texture {index} of {what}"),
        );
        match held[source] {
            Some(previous) => assert_eq!(
                previous, sampler,
                "the image {source} of {what} holds two samplers",
            ),
            None => {
                held[source] = Some(sampler);
                samplers[source] = sampler;
            }
        }
    }
    samplers
}

fn read(document: &Document, index: Option<usize>, what: &str) -> Sampler {
    let Some(index) = index else {
        return default_sampler();
    };
    let sampler = document.samplers.get(index).unwrap_or_else(|| {
        panic!("{what} names the sampler {index}, which the model does not hold")
    });
    let mag_filter = sampler.mag_filter.map_or(Filter::Linear, |value| {
        mag_filter(value, &format!("the mag filter of {what}"))
    });
    let (min_filter, mip_filter) = sampler
        .min_filter
        .map_or((Filter::Linear, Filter::Linear), |value| {
            min_filter(value, &format!("the min filter of {what}"))
        });
    Sampler {
        min_filter,
        mag_filter,
        mip_filter,
        wrap_u: sampler.wrap_s.map_or(Wrap::Repeat, |value| {
            wrap(value, &format!("the wrap s of {what}"))
        }),
        wrap_v: sampler.wrap_t.map_or(Wrap::Repeat, |value| {
            wrap(value, &format!("the wrap t of {what}"))
        }),
    }
}

fn mag_filter(value: u32, what: &str) -> Filter {
    match value {
        9728 => Filter::Nearest,
        9729 => Filter::Linear,
        other => panic!("{what} holds the filter {other}"),
    }
}

fn min_filter(value: u32, what: &str) -> (Filter, Filter) {
    match value {
        9728 => (Filter::Nearest, Filter::Nearest),
        9729 => (Filter::Linear, Filter::Linear),
        9984 => (Filter::Nearest, Filter::Nearest),
        9985 => (Filter::Linear, Filter::Nearest),
        9986 => (Filter::Nearest, Filter::Linear),
        9987 => (Filter::Linear, Filter::Linear),
        other => panic!("{what} holds the filter {other}"),
    }
}

fn wrap(value: u32, what: &str) -> Wrap {
    match value {
        33071 => Wrap::ClampToEdge,
        33648 => Wrap::MirroredRepeat,
        10497 => Wrap::Repeat,
        other => panic!("{what} holds the wrap {other}"),
    }
}

fn default_sampler() -> Sampler {
    Sampler {
        min_filter: Filter::Linear,
        mag_filter: Filter::Linear,
        mip_filter: Filter::Linear,
        wrap_u: Wrap::Repeat,
        wrap_v: Wrap::Repeat,
    }
}
