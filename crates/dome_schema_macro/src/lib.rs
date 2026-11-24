use proc_macro::TokenStream;
use proc_macro2::{Literal, Span};
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DeriveInput, Expr, ExprArray, ExprLit, ExprUnary, Field, Fields, Lit, LitStr,
    Token, UnOp, parse_macro_input,
};

#[derive(Default)]
struct ComponentOptions {
    name: Option<(String, Span)>,
    label: Option<(String, Span)>,
    required: bool,
    reach: Option<(Reach, Span)>,
}

#[derive(Clone, Copy)]
enum Reach {
    None,
    Node,
    World,
}

#[derive(Default)]
struct FieldOptions {
    label: Option<(String, Span)>,
    range: Option<([f32; 2], Span)>,
    speed: Option<(f32, Span)>,
    choices: Option<(Vec<String>, Span)>,
    color: bool,
    runtime: bool,
    skip: bool,
}

#[proc_macro_derive(SceneComponent, attributes(scene))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "a scene component is a struct",
        ));
    };
    let fields = match &data.fields {
        Fields::Named(fields) => fields.named.iter().collect::<Vec<_>>(),
        Fields::Unit => Vec::new(),
        Fields::Unnamed(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "a scene component holds named fields or no fields",
            ));
        }
    };
    let options = component_options(&input.attrs)?;
    let name = options
        .name
        .map(|(name, _)| name)
        .unwrap_or_else(|| snake(&input.ident.to_string()));
    let label = options
        .label
        .map(|(label, _)| label)
        .unwrap_or_else(|| title(&name));
    let mut field_kinds = Vec::new();
    let mut runtime = Vec::new();
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut authored = Vec::new();
    let mut observed = Vec::new();
    let mut runtime_values = Vec::new();
    let mut parses = Vec::new();
    let mut validates = Vec::new();
    let mut references = Vec::new();
    let mut checks = Vec::new();
    let mut initializers = Vec::new();
    let mut creations = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let field = *field;
        let ty = &field.ty;
        let options = field_options(field)?;
        let ident = field
            .ident
            .as_ref()
            .expect("a named field holds its identifier");
        let field_name = ident.to_string();
        let field_label = options
            .label
            .as_ref()
            .map(|(label, _)| label.clone())
            .unwrap_or_else(|| title(&field_name));
        let kind = kind(ty, &field_label, &options, field)?;
        let local = format_ident!("__field_{index}");
        if !options.skip {
            field_kinds.push(quote! {
                ::dome_scene::__private::Field {
                    name: #field_name,
                    label: #field_label,
                    kind: #kind,
                }
            });
            reads.push(quote! {
                #field_name => Some(<#ty as ::dome_scene::SceneValue>::read(&self.#ident)),
            });
            authored.push(quote! {
                (#field_name, <#ty as ::dome_scene::SceneValue>::authored(&self.#ident)),
            });
            observed.push(quote! {
                (#field_name, <#ty as ::dome_scene::SceneValue>::authored(&self.#ident)),
            });
            parses.push(quote! {
                let #local = <#ty as ::dome_scene::SceneValue>::parse(
                    __fields.need(#field_name)?,
                    &format!("the {} of {what}", #field_name),
                )?;
            });
        }
        validates.push(quote! {
            <#ty as ::dome_scene::SceneValue>::validate(
                &self.#ident,
                scene,
                &format!("the {} of node {node}", #field_name),
            )?;
        });
        references.push(quote! {
            <#ty as ::dome_scene::SceneValue>::references(&self.#ident, kind, id)
        });
        if options.skip {
            initializers.push(quote! {
                #ident: ::core::default::Default::default(),
            });
        } else {
            initializers.push(quote! {
                #ident: #local,
            });
            creations.push(quote! {
                if <#ty as ::dome_scene::SceneValue>::validate(
                    &component.#ident,
                    scene,
                    "a created scene component",
                )
                .is_err()
                {
                    component.#ident = <#ty as ::dome_scene::SceneValue>::create(scene)?;
                }
            });
        }
        if options.runtime {
            runtime.push(quote! {
                #field_name
            });
            runtime_values.push(quote! {
                (#field_name, <#ty as ::dome_scene::SceneValue>::authored(&self.#ident)),
            });
        }
        if let Some((choices, span)) = &options.choices {
            let literals = choices
                .iter()
                .map(|choice| LitStr::new(choice, *span))
                .collect::<Vec<_>>();
            if !options.skip {
                checks.push(quote! {
                    if !matches!(__component.#ident.as_str(), #(#literals)|*) {
                        return ::core::result::Result::Err(format!(
                            "{what} holds the {} {}",
                            #field_name,
                            __component.#ident,
                        ));
                    }
                });
            }
            validates.push(quote! {
                if !matches!(self.#ident.as_str(), #(#literals)|*) {
                    return Err(format!(
                        "the {} of node {node} holds {}",
                        #field_name,
                        self.#ident,
                    ));
                }
            });
            if !options.skip || options.runtime {
                writes.push(quote! {
                    #field_name => {
                        let __next = <#ty as ::dome_scene::SceneValue>::write(
                            &self.#ident,
                            axis,
                            value,
                            &format!("the {} of {}", #field_name, <Self as ::dome_schema::SceneComponent>::SPEC.name),
                        )?;
                        if !matches!(__next.as_str(), #(#literals)|*) {
                            return Err(format!(
                                "the {} of {} holds {}",
                                #field_name,
                                <Self as ::dome_schema::SceneComponent>::SPEC.name,
                                __next,
                            ));
                        }
                        self.#ident = __next;
                    }
                });
            }
        } else if !options.skip || options.runtime {
            writes.push(quote! {
                #field_name => {
                    let __next = <#ty as ::dome_scene::SceneValue>::write(
                        &self.#ident,
                        axis,
                        value,
                        &format!("the {} of {}", #field_name, <Self as ::dome_schema::SceneComponent>::SPEC.name),
                    )?;
                    self.#ident = __next;
                }
            });
        }
    }
    let ident = &input.ident;
    let required = options.required;
    let authored_object = if authored.is_empty() {
        quote! {
            ::dome_scene::__private::Toml::object(
                ::core::iter::empty::<(&str, ::dome_scene::__private::Toml)>(),
            )
        }
    } else {
        quote! {
            ::dome_scene::__private::Toml::object([#(#authored)*])
        }
    };
    let observed_object = if observed.is_empty() {
        quote! {
            ::dome_scene::__private::Toml::object(
                ::core::iter::empty::<(&str, ::dome_scene::__private::Toml)>(),
            )
        }
    } else {
        quote! {
            ::dome_scene::__private::Toml::object([#(#observed)*])
        }
    };
    let runtime_object = if runtime_values.is_empty() {
        quote! {
            ::dome_scene::__private::Toml::object(
                ::core::iter::empty::<(&str, ::dome_scene::__private::Toml)>(),
            )
        }
    } else {
        quote! {
            ::dome_scene::__private::Toml::object([#(#runtime_values)*])
        }
    };
    let read = if reads.is_empty() {
        quote! {
            fn read(&self, _field: &str) -> ::core::option::Option<::dome_scene::__private::Value> {
                ::core::option::Option::None
            }
        }
    } else {
        quote! {
            fn read(&self, field: &str) -> ::core::option::Option<::dome_scene::__private::Value> {
                match field {
                    #(#reads)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    };
    let write = if writes.is_empty() {
        quote! {
            fn write(
                &mut self,
                field: &str,
                _axis: ::core::option::Option<::dome_scene::__private::Axis>,
                _value: &::dome_scene::__private::Value,
            ) -> ::core::result::Result<::dome_scene::__private::Effect, ::std::string::String> {
                ::core::result::Result::Err(format!(
                    "the {} holds no property {field}",
                    <Self as ::dome_schema::SceneComponent>::SPEC.name,
                ))
            }
        }
    } else {
        quote! {
            fn write(
                &mut self,
                field: &str,
                axis: ::core::option::Option<::dome_scene::__private::Axis>,
                value: &::dome_scene::__private::Value,
            ) -> ::core::result::Result<::dome_scene::__private::Effect, ::std::string::String> {
                match field {
                    #(#writes)*
                    _ => {
                        return ::core::result::Result::Err(format!(
                            "the {} holds no property {field}",
                            <Self as ::dome_schema::SceneComponent>::SPEC.name,
                        ));
                    }
                }
                ::core::result::Result::Ok(::dome_scene::__private::Effect::Value)
            }
        }
    };
    let runtime_fields = quote! { &[#(#runtime),*] };
    let reach = match options.reach.map(|(reach, _)| reach).unwrap_or(Reach::None) {
        Reach::None => quote! { ::dome_scene::__private::Reach::None },
        Reach::Node => quote! { ::dome_scene::__private::Reach::Node },
        Reach::World => quote! { ::dome_scene::__private::Reach::World },
    };
    let validate_choice = if checks.is_empty() {
        quote! {}
    } else {
        quote! {
            let __component = Self { #(#initializers)* };
            #(#checks)*
            ::core::result::Result::Ok(__component)
        }
    };
    let parse = if checks.is_empty() {
        quote! {
            let mut __fields = ::dome_scene::__private::Fields::of(value, what)?;
            #(#parses)*
            __fields.done()?;
            ::core::result::Result::Ok(Self { #(#initializers)* })
        }
    } else {
        quote! {
            let mut __fields = ::dome_scene::__private::Fields::of(value, what)?;
            #(#parses)*
            __fields.done()?;
            #validate_choice
        }
    };
    Ok(quote! {
        impl ::dome_schema::SceneComponent for #ident {
            const SPEC: &'static ::dome_scene::__private::ComponentSpec = {
                const FIELDS: &[::dome_scene::__private::Field] = &[
                    #(#field_kinds),*
                ];
                const RUNTIME_FIELDS: &[&str] = #runtime_fields;
                &::dome_scene::__private::ComponentSpec {
                    name: #name,
                    label: #label,
                    required: #required,
                    fields: FIELDS,
                    runtime_fields: RUNTIME_FIELDS,
                    reach: #reach,
                }
            };

            #read

            #write

            fn authored(&self) -> ::dome_scene::__private::Toml {
                #authored_object
            }

            fn observed(&self) -> ::dome_scene::__private::Toml {
                #observed_object
            }

            fn runtime(&self) -> ::dome_scene::__private::Toml {
                #runtime_object
            }

            fn install(
                &self,
                _scene: &::dome_scene::Scene,
                _node: &str,
                world: &mut ::dome_schema::World,
                entity: ::dome_schema::Entity,
                _assets: &dyn ::dome_schema::Assets,
            ) {
                world.insert(entity, self.clone());
            }

            fn validate(
                &self,
                scene: &::dome_scene::Scene,
                node: &str,
            ) -> ::core::result::Result<(), ::std::string::String> {
                #(#validates)*
                ::core::result::Result::Ok(())
            }

            fn create(
                scene: &::dome_scene::Scene,
            ) -> ::core::result::Result<Self, ::std::string::String> {
                let mut component = <Self as ::core::default::Default>::default();
                #(#creations)*
                ::core::result::Result::Ok(component)
            }

            fn from_toml(
                value: ::dome_scene::__private::Toml,
                what: &str,
            ) -> ::core::result::Result<Self, ::std::string::String> {
                #parse
            }

            fn observe(
                world: &::dome_schema::World,
                entity: ::dome_schema::Entity,
            ) -> ::core::option::Option<Self> {
                world.get::<Self>(entity).cloned()
            }

            fn remove(
                world: &mut ::dome_schema::World,
                entity: ::dome_schema::Entity,
            ) {
                world.remove::<Self>(entity);
            }

            fn references(
                &self,
                kind: ::dome_scene::ReferenceKind,
                id: &str,
            ) -> bool {
                false #(|| #references)*
            }

            fn write_world(
                world: &mut ::dome_schema::World,
                entity: ::dome_schema::Entity,
                field: &str,
                axis: ::core::option::Option<::dome_scene::__private::Axis>,
                value: &::dome_scene::__private::Value,
            ) -> ::core::result::Result<(), ::std::string::String> {
                if !<Self as ::dome_schema::SceneComponent>::SPEC.runtime_fields.contains(&field) {
                    return ::core::result::Result::Err(format!(
                        "the runtime {} holds no property {field}",
                        <Self as ::dome_schema::SceneComponent>::SPEC.name,
                    ));
                }
                let mut __component = world.get::<Self>(entity).cloned().ok_or_else(|| {
                    format!(
                        "the runtime node {} holds no {}",
                        entity.index(),
                        <Self as ::dome_schema::SceneComponent>::SPEC.name,
                    )
                })?;
                <Self as ::dome_schema::SceneComponent>::write(
                    &mut __component,
                    field,
                    axis,
                    value,
                )?;
                world.insert(entity, __component);
                ::core::result::Result::Ok(())
            }
        }
    })
}

fn component_options(attrs: &[Attribute]) -> syn::Result<ComponentOptions> {
    let mut options = ComponentOptions::default();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("scene")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let value = meta.value()?.parse::<LitStr>()?;
                set(&mut options.name, value.value(), value.span(), "name")?;
            } else if meta.path.is_ident("label") {
                let value = meta.value()?.parse::<LitStr>()?;
                set(&mut options.label, value.value(), value.span(), "label")?;
            } else if meta.path.is_ident("required") {
                flag(&meta, "required")?;
                if options.required {
                    return Err(meta.error("the scene component repeats required"));
                }
                options.required = true;
            } else if meta.path.is_ident("reach") {
                let value = meta.value()?.parse::<LitStr>()?;
                let reach = match value.value().as_str() {
                    "none" => Reach::None,
                    "node" => Reach::Node,
                    "world" => Reach::World,
                    other => {
                        return Err(syn::Error::new(
                            value.span(),
                            format!("a scene component reaches none, node or world, not {other}"),
                        ));
                    }
                };
                set(&mut options.reach, reach, value.span(), "reach")?;
            } else {
                return Err(meta.error("a scene component holds name, label, required or reach"));
            }
            Ok(())
        })?;
    }
    Ok(options)
}

fn field_options(field: &Field) -> syn::Result<FieldOptions> {
    let mut options = FieldOptions::default();
    for attr in field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("scene"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("label") {
                let value = meta.value()?.parse::<LitStr>()?;
                set(&mut options.label, value.value(), value.span(), "label")?;
            } else if meta.path.is_ident("range") {
                let value = meta.value()?.parse::<ExprArray>()?;
                let range = range(&value)?;
                set(
                    &mut options.range,
                    range,
                    value.bracket_token.span.join(),
                    "range",
                )?;
            } else if meta.path.is_ident("speed") {
                let value: Expr = meta.value()?.parse()?;
                set(&mut options.speed, number(&value)?, value.span(), "speed")?;
            } else if meta.path.is_ident("choices") {
                let value = meta.value()?.parse::<ExprArray>()?;
                let choices = choices(&value)?;
                set(
                    &mut options.choices,
                    choices,
                    value.bracket_token.span.join(),
                    "choices",
                )?;
            } else if meta.path.is_ident("color") {
                flag(&meta, "color")?;
                if options.color {
                    return Err(meta.error("the scene field repeats color"));
                }
                options.color = true;
            } else if meta.path.is_ident("runtime") {
                flag(&meta, "runtime")?;
                if options.runtime {
                    return Err(meta.error("the scene field repeats runtime"));
                }
                options.runtime = true;
            } else if meta.path.is_ident("skip") {
                flag(&meta, "skip")?;
                if options.skip {
                    return Err(meta.error("the scene field repeats skip"));
                }
                options.skip = true;
            } else {
                return Err(meta.error(
                    "a scene field holds label, range, speed, choices, color, runtime or skip",
                ));
            }
            Ok(())
        })?;
    }
    Ok(options)
}

fn set<T>(slot: &mut Option<(T, Span)>, value: T, span: Span, name: &str) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new(
            span,
            format!("the scene attribute repeats {name}"),
        ));
    }
    *slot = Some((value, span));
    Ok(())
}

fn flag(meta: &syn::meta::ParseNestedMeta<'_>, name: &str) -> syn::Result<()> {
    if meta.input.peek(Token![=]) {
        return Err(meta.error(format!("{name} holds no value")));
    }
    Ok(())
}

fn range(value: &ExprArray) -> syn::Result<[f32; 2]> {
    if value.elems.len() != 2 {
        return Err(syn::Error::new_spanned(
            value,
            "a scene range holds two numbers",
        ));
    }
    let mut numbers = value.elems.iter().map(number);
    let low = numbers.next().expect("a counted range holds its low")?;
    let high = numbers.next().expect("a counted range holds its high")?;
    if !(low.is_finite() && high.is_finite()) || low > high {
        return Err(syn::Error::new_spanned(
            value,
            "a scene range holds finite numbers in order",
        ));
    }
    Ok([low, high])
}

fn number(value: &Expr) -> syn::Result<f32> {
    match value {
        Expr::Lit(ExprLit {
            lit: Lit::Float(lit),
            ..
        }) => lit.base10_parse::<f32>(),
        Expr::Lit(ExprLit {
            lit: Lit::Int(lit), ..
        }) => lit.base10_parse::<f32>(),
        Expr::Unary(ExprUnary {
            op: UnOp::Neg(_),
            expr,
            ..
        }) => Ok(-number(expr)?),
        other => Err(syn::Error::new_spanned(
            other,
            "a scene number is a literal",
        )),
    }
}

fn choices(value: &ExprArray) -> syn::Result<Vec<String>> {
    if value.elems.is_empty() {
        return Err(syn::Error::new_spanned(
            value,
            "a scene choice holds at least one text",
        ));
    }
    value
        .elems
        .iter()
        .map(|item| match item {
            Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) => Ok(text.value()),
            other => Err(syn::Error::new_spanned(
                other,
                "a scene choice is a text literal",
            )),
        })
        .collect()
}

fn is_string(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "String")
}

fn kind(
    ty: &syn::Type,
    label: &str,
    options: &FieldOptions,
    field: &Field,
) -> syn::Result<proc_macro2::TokenStream> {
    let span = field
        .ident
        .as_ref()
        .map_or_else(Span::call_site, |ident| ident.span());
    if options.color && (options.range.is_some() || options.speed.is_some()) {
        return Err(syn::Error::new(
            span,
            format!("the scene field {label} holds a color"),
        ));
    }
    if options.choices.is_some() && !is_string(ty) {
        return Err(syn::Error::new(
            span,
            format!("the scene field {label} holds text choices"),
        ));
    }
    let mut expression = quote! {
        <#ty as ::dome_scene::SceneValue>::KIND
    };
    if let Some((range, _)) = options.range {
        let low = literal(range[0]);
        let high = literal(range[1]);
        expression = quote! {
            #expression.with_range([#low, #high])
        };
    }
    if let Some((speed, _)) = options.speed {
        let speed = literal(speed);
        expression = quote! {
            #expression.with_speed(#speed)
        };
    }
    if let Some((choices, choice_span)) = &options.choices {
        let literals = choices
            .iter()
            .map(|choice| LitStr::new(choice, *choice_span))
            .collect::<Vec<_>>();
        expression = quote! {
            #expression.with_choices(&[#(#literals),*])
        };
    }
    if options.color {
        expression = quote! {
            #expression.with_color()
        };
    }
    Ok(expression)
}

fn literal(value: f32) -> Literal {
    Literal::f32_suffixed(value)
}

fn snake(name: &str) -> String {
    let mut output = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn title(name: &str) -> String {
    name.split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => {
                    format!("{}{}", first.to_uppercase(), characters.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
