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

pub(crate) fn image_samplers(document: &Document, what: &str) -> Result<Vec<Sampler>, String> {
    let mut samplers = vec![default_sampler(); document.images.len()];
    let mut held = vec![None; document.images.len()];
    for (index, texture) in document.textures.iter().enumerate() {
        let Some(source) = texture.source else {
            continue;
        };
        if source >= document.images.len() {
            return Err(format!(
                "texture {index} of {what} names the image {source}, which the model does not hold"
            ));
        }
        let sampler = read(
            document,
            texture.sampler,
            &format!("texture {index} of {what}"),
        )?;
        match held[source] {
            Some(previous) if previous != sampler => {
                return Err(format!("the image {source} of {what} holds two samplers"));
            }
            Some(_) => {}
            None => {
                held[source] = Some(sampler);
                samplers[source] = sampler;
            }
        }
    }
    Ok(samplers)
}

fn read(document: &Document, index: Option<usize>, what: &str) -> Result<Sampler, String> {
    let Some(index) = index else {
        return Ok(default_sampler());
    };
    let sampler = document.samplers.get(index).ok_or_else(|| {
        format!("{what} names the sampler {index}, which the model does not hold")
    })?;
    let mag_filter = sampler
        .mag_filter
        .map(|value| mag_filter(value, &format!("the mag filter of {what}")))
        .transpose()?
        .unwrap_or(Filter::Linear);
    let (min_filter, mip_filter) = sampler
        .min_filter
        .map(|value| min_filter(value, &format!("the min filter of {what}")))
        .transpose()?
        .unwrap_or((Filter::Linear, Filter::Linear));
    Ok(Sampler {
        min_filter,
        mag_filter,
        mip_filter,
        wrap_u: sampler
            .wrap_s
            .map(|value| wrap(value, &format!("the wrap s of {what}")))
            .transpose()?
            .unwrap_or(Wrap::Repeat),
        wrap_v: sampler
            .wrap_t
            .map(|value| wrap(value, &format!("the wrap t of {what}")))
            .transpose()?
            .unwrap_or(Wrap::Repeat),
    })
}

fn mag_filter(value: u32, what: &str) -> Result<Filter, String> {
    match value {
        9728 => Ok(Filter::Nearest),
        9729 => Ok(Filter::Linear),
        other => Err(format!("{what} holds the filter {other}")),
    }
}

fn min_filter(value: u32, what: &str) -> Result<(Filter, Filter), String> {
    match value {
        9728 => Ok((Filter::Nearest, Filter::Nearest)),
        9729 => Ok((Filter::Linear, Filter::Linear)),
        9984 => Ok((Filter::Nearest, Filter::Nearest)),
        9985 => Ok((Filter::Linear, Filter::Nearest)),
        9986 => Ok((Filter::Nearest, Filter::Linear)),
        9987 => Ok((Filter::Linear, Filter::Linear)),
        other => Err(format!("{what} holds the filter {other}")),
    }
}

fn wrap(value: u32, what: &str) -> Result<Wrap, String> {
    match value {
        33071 => Ok(Wrap::ClampToEdge),
        33648 => Ok(Wrap::MirroredRepeat),
        10497 => Ok(Wrap::Repeat),
        other => Err(format!("{what} holds the wrap {other}")),
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
