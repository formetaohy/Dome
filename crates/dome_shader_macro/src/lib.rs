mod lower;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use std::collections::BTreeMap;
use syn::parse::{Parse, ParseStream, Parser};
use syn::visit_mut::{self, VisitMut};
use syn::{FnArg, GenericArgument, Pat, PathArguments, Token};

struct KernelOptions {
    size: u32,
}

impl Parse for KernelOptions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        if name != "workgroup_size" {
            return Err(syn::Error::new_spanned(
                name,
                "kernel attributes specify workgroup_size",
            ));
        }
        input.parse::<Token![=]>()?;
        let size: syn::LitInt = input.parse()?;
        let size = size.base10_parse::<u32>()?;
        if size == 0 || !input.is_empty() {
            return Err(input.error("a kernel has one positive workgroup size"));
        }
        Ok(Self { size })
    }
}

struct Shared {
    name: syn::Ident,
    element: syn::Ident,
    count: u32,
}

struct ShaderModule {
    name: syn::Ident,
    visibility: syn::Visibility,
    shared: Vec<Shared>,
    functions: Vec<syn::ItemFn>,
    entry: Option<syn::Ident>,
}

fn is_entry_marker(attr: &syn::Attribute) -> bool {
    let segments = &attr.path().segments;
    segments.last().is_some_and(|part| part.ident == "kernel")
        && (segments.len() == 1 || segments.len() == 2 && segments[0].ident == "dome_shader")
}

fn shared_memory(item: syn::ItemMacro) -> syn::Result<Shared> {
    if !item.mac.path.is_ident("workgroup") {
        return Err(syn::Error::new_spanned(
            &item.mac.path,
            "a shader module declares workgroup memory with workgroup!(NAME: [element; count])",
        ));
    }
    let declaration = item.mac.tokens.clone();
    let parse = |input: ParseStream<'_>| {
        let name: syn::Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: syn::Type = input.parse()?;
        Ok((name, ty))
    };
    let (name, ty) = parse.parse2(declaration)?;
    let syn::Type::Array(array) = &ty else {
        return Err(syn::Error::new_spanned(
            &ty,
            "shader workgroup memory is an array",
        ));
    };
    let syn::Type::Path(path) = array.elem.as_ref() else {
        return Err(syn::Error::new_spanned(
            array.elem.as_ref(),
            "shader workgroup memory holds u32, i32 or f32",
        ));
    };
    let Some(element) = path.path.get_ident().cloned() else {
        return Err(syn::Error::new_spanned(
            array.elem.as_ref(),
            "shader workgroup memory holds u32, i32 or f32",
        ));
    };
    if !matches!(element.to_string().as_str(), "u32" | "i32" | "f32") {
        return Err(syn::Error::new_spanned(
            element,
            "shader workgroup memory holds u32, i32 or f32",
        ));
    }
    if name.to_string().chars().any(char::is_uppercase) {
        return Err(syn::Error::new_spanned(
            &name,
            "shader workgroup memory uses a snake case name",
        ));
    }
    let syn::Expr::Lit(literal) = &array.len else {
        return Err(syn::Error::new_spanned(
            &array.len,
            "a shader workgroup array counts a literal element count",
        ));
    };
    let syn::Lit::Int(count) = &literal.lit else {
        return Err(syn::Error::new_spanned(
            &literal.lit,
            "a shader workgroup array counts a literal element count",
        ));
    };
    let count = count.base10_parse::<u32>()?;
    if count == 0 {
        return Err(syn::Error::new_spanned(
            &name,
            "a shader workgroup array holds at least one element",
        ));
    }
    Ok(Shared {
        name,
        element,
        count,
    })
}

fn shader_module(input: syn::ItemMod) -> syn::Result<ShaderModule> {
    let Some((_, items)) = input.content else {
        return Err(syn::Error::new_spanned(
            input,
            "a kernel module is defined inline",
        ));
    };
    if let Some(attribute) = input.attrs.first() {
        return Err(syn::Error::new_spanned(
            attribute,
            "a kernel module carries no attribute of its own",
        ));
    }
    let visibility = input.vis;
    let name = input.ident;
    let mut shared = Vec::new();
    let mut functions = Vec::new();
    let mut entry = None;
    for item in items {
        match item {
            syn::Item::Fn(mut function) => {
                let mut marked = false;
                for attr in &function.attrs {
                    if !is_entry_marker(attr) {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "kernel module functions only use #[kernel]",
                        ));
                    }
                    if !matches!(attr.meta, syn::Meta::Path(_)) || marked {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "a shader module declares one #[kernel] entry",
                        ));
                    }
                    if !matches!(function.sig.output, syn::ReturnType::Default) {
                        return Err(syn::Error::new_spanned(
                            &function.sig.output,
                            "compute entries do not return values",
                        ));
                    }
                    marked = true;
                }
                function.attrs.clear();
                if marked {
                    if entry.is_some() {
                        return Err(syn::Error::new_spanned(
                            &function.sig.ident,
                            "a shader module declares one #[kernel] entry",
                        ));
                    }
                    entry = Some(function.sig.ident.clone());
                }
                functions.push(function);
            }
            syn::Item::Macro(item) => shared.push(shared_memory(item)?),
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "a kernel module contains Rust functions and workgroup arrays",
                ));
            }
        }
    }
    if functions.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "a kernel module needs functions",
        ));
    }
    Ok(ShaderModule {
        name,
        visibility,
        shared,
        functions,
        entry,
    })
}

fn shared_declarations(shared: &[Shared]) -> Vec<Tokens> {
    shared
        .iter()
        .map(|shared| {
            let name = shared.name.to_string();
            let element = shared.element.to_string();
            let count = shared.count;
            quote!(compiler.workgroup(#name, #element, #count);)
        })
        .collect()
}

fn shared_locals(source: &syn::ItemFn, shared: &[Shared]) -> Vec<syn::Stmt> {
    shared
        .iter()
        .filter_map(|shared| {
            let mut scan = Mentions {
                name: &shared.name,
                present: false,
                written: false,
            };
            let mut function = source.clone();
            scan.visit_item_fn_mut(&mut function);
            if !scan.present {
                return None;
            }
            let name = &shared.name;
            let element = &shared.element;
            let count = shared.count as usize;
            Some(if scan.written {
                syn::parse_quote!(let mut #name = ::dome_shader::Workgroup::<#element, #count>::new();)
            } else {
                syn::parse_quote!(let #name = ::dome_shader::Workgroup::<#element, #count>::new();)
            })
        })
        .collect()
}

struct Mentions<'a> {
    name: &'a syn::Ident,
    present: bool,
    written: bool,
}

impl Mentions<'_> {
    fn mark(&mut self, place: &mut syn::Expr) {
        let present = self.present;
        self.present = false;
        self.visit_expr_mut(place);
        if self.present {
            self.written = true;
        }
        self.present |= present;
    }
}

impl VisitMut for Mentions<'_> {
    fn visit_ident_mut(&mut self, ident: &mut syn::Ident) {
        if ident == self.name {
            self.present = true;
        }
    }

    fn visit_expr_assign_mut(&mut self, assignment: &mut syn::ExprAssign) {
        self.mark(&mut assignment.left);
        visit_mut::visit_expr_assign_mut(self, assignment);
    }

    fn visit_expr_binary_mut(&mut self, binary: &mut syn::ExprBinary) {
        if matches!(
            binary.op,
            syn::BinOp::AddAssign(_)
                | syn::BinOp::SubAssign(_)
                | syn::BinOp::MulAssign(_)
                | syn::BinOp::DivAssign(_)
                | syn::BinOp::RemAssign(_)
                | syn::BinOp::BitAndAssign(_)
                | syn::BinOp::BitOrAssign(_)
                | syn::BinOp::BitXorAssign(_)
                | syn::BinOp::ShlAssign(_)
                | syn::BinOp::ShrAssign(_)
        ) {
            self.mark(&mut binary.left);
        }
        visit_mut::visit_expr_binary_mut(self, binary);
    }
}

fn expand_definitions(module: ShaderModule) -> syn::Result<Tokens> {
    let ShaderModule {
        name,
        visibility,
        shared,
        functions,
        entry,
    } = module;
    let declarations = shared_declarations(&shared);
    let definitions = functions
        .iter()
        .map(lower::function)
        .collect::<syn::Result<Vec<_>>>()?;
    let uses_entry = entry.is_some().then(|| quote!(let _ = ENTRY;));
    let entry = entry.map(|entry| {
        quote!(
            pub const ENTRY: &str = stringify!(#entry);
        )
    });
    Ok(quote! {
        #visibility mod #name {
            #entry
            pub(crate) fn define(compiler: &mut ::dome_shader::Compiler) {
                use ::dome_shader::ast as dome_ast;
                #uses_entry
                #(#declarations)*
                #(compiler.function(#definitions);)*
            }
        }
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryStage {
    Compute,
    Vertex,
    Fragment,
}

struct Storage {
    name: String,
    element: String,
    writable: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResourceKind {
    Texture,
    Sampler,
    TextureTable,
    SamplerTable,
}

struct Resource {
    name: String,
    kind: ResourceKind,
}

struct Varying {
    name: String,
    element: String,
    output: bool,
    position: bool,
}

struct Bindings {
    builtins: syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    typecheck: syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    outputs: Vec<String>,
    records: BTreeMap<String, syn::Ident>,
    storages: Vec<Storage>,
    resources: Vec<Resource>,
    varyings: Vec<Varying>,
}

fn mark_typed_mutable(arg: &mut syn::PatType) {
    let Pat::Ident(variable) = arg.pat.as_mut() else {
        return;
    };
    variable.mutability = Some(syn::Token![mut](variable.ident.span()));
}

fn check_body(function: &syn::ItemFn, bindings: &Bindings) -> syn::ItemFn {
    let mut check = function.clone();
    check.sig.inputs = bindings.typecheck.clone();
    check.vis = syn::Visibility::Inherited;
    for output in &bindings.outputs {
        let output = syn::Ident::new(output, proc_macro2::Span::call_site());
        check
            .block
            .stmts
            .push(syn::parse_quote!(::std::hint::black_box(#output);));
    }
    for output in bindings.outputs.iter().rev() {
        let output = syn::Ident::new(output, proc_macro2::Span::call_site());
        check
            .block
            .stmts
            .insert(0, syn::parse_quote!(::std::hint::black_box(&#output);));
    }
    check
}

fn storage_path(ty: &syn::Type) -> Option<&syn::PathSegment> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let segments = path.path.segments.iter().collect::<Vec<_>>();
    let [segment] = segments.as_slice() else {
        return None;
    };
    matches!(segment.ident.to_string().as_str(), "Read" | "ReadWrite").then_some(*segment)
}

fn varying_argument(ty: &syn::Type) -> Option<(bool, syn::Type)> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let output = match segment.ident.to_string().as_str() {
        "Out" => true,
        "In" => false,
        _ => return None,
    };
    let PathArguments::AngleBracketed(generics) = &segment.arguments else {
        return None;
    };
    let [GenericArgument::Type(element)] = generics.args.iter().collect::<Vec<_>>().as_slice()
    else {
        return None;
    };
    Some((output, element.clone()))
}

fn varying_element(element: &syn::Type, stage: EntryStage) -> syn::Result<(String, bool)> {
    let syn::Type::Path(path) = element else {
        return Err(syn::Error::new_spanned(
            element,
            "varyings hold fvec4, fvec2, f32, u32 or Position",
        ));
    };
    let Some(ident) = path.path.get_ident() else {
        return Err(syn::Error::new_spanned(
            element,
            "varyings hold fvec4, fvec2, f32, u32 or Position",
        ));
    };
    match ident.to_string().as_str() {
        "Position" => {
            if stage != EntryStage::Vertex {
                return Err(syn::Error::new_spanned(
                    ident,
                    "only a vertex stage writes the clip space position",
                ));
            }
            Ok(("fvec4".to_owned(), true))
        }
        "Fvec4" => Ok(("fvec4".to_owned(), false)),
        "Fvec2" => Ok(("fvec2".to_owned(), false)),
        "F32" | "f32" => Ok(("f32".to_owned(), false)),
        "U32" | "u32" => Ok(("u32".to_owned(), false)),
        _ => Err(syn::Error::new_spanned(
            ident,
            "varyings hold fvec4, fvec2, f32, u32 or Position",
        )),
    }
}

fn resource_kind(ty: &syn::Type) -> Option<ResourceKind> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    match path.path.get_ident()?.to_string().as_str() {
        "Texture2D" => Some(ResourceKind::Texture),
        "Sampler" => Some(ResourceKind::Sampler),
        "TextureTable" => Some(ResourceKind::TextureTable),
        "SamplerTable" => Some(ResourceKind::SamplerTable),
        _ => None,
    }
}

fn storage_element(ty: &syn::Type) -> syn::Result<String> {
    let Some(segment) = storage_path(ty) else {
        return Err(syn::Error::new_spanned(
            ty,
            "storage arguments use Read<T> or ReadWrite<T>",
        ));
    };
    let PathArguments::AngleBracketed(generics) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            ty,
            "storage arguments specify an element type",
        ));
    };
    let arguments = generics.args.iter().collect::<Vec<_>>();
    let [GenericArgument::Type(element)] = arguments.as_slice() else {
        return Err(syn::Error::new_spanned(
            generics,
            "storage buffers have one scalar element type",
        ));
    };
    let syn::Type::Path(path) = element else {
        return Err(syn::Error::new_spanned(
            element,
            "storage buffers contain u32, f32, AtomicU32 or an ABI record",
        ));
    };
    let Some(ident) = path.path.get_ident() else {
        return Err(syn::Error::new_spanned(
            element,
            "storage buffers contain u32, f32, AtomicU32 or an ABI record",
        ));
    };
    let name = ident.to_string();
    if name == "AtomicU32" || name == "u32" || name == "f32" || name.ends_with("Record") {
        Ok(name)
    } else {
        Err(syn::Error::new_spanned(
            ident,
            "storage buffers contain u32, f32, AtomicU32 or an ABI record",
        ))
    }
}

fn bindings(entry: &mut syn::ItemFn, stage: EntryStage) -> syn::Result<Bindings> {
    let mut builtins = syn::punctuated::Punctuated::new();
    let mut typecheck = syn::punctuated::Punctuated::new();
    let mut records = BTreeMap::new();
    let mut storages = Vec::new();
    let mut resources = Vec::new();
    let mut varyings = Vec::new();
    let mut outputs = Vec::new();
    let source_function = entry.clone();
    for input in &mut entry.sig.inputs {
        let FnArg::Typed(arg) = input else {
            return Err(syn::Error::new_spanned(input, "kernel arguments are named"));
        };
        let Pat::Ident(variable) = arg.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                arg,
                "kernel arguments have simple names",
            ));
        };
        let argument_name = variable.ident.to_string();
        let source = arg.clone();
        if matches!(
            argument_name.as_str(),
            "lid" | "group" | "global" | "vertex" | "instance"
        ) {
            let expected = if matches!(argument_name.as_str(), "group" | "global") {
                "Uvec3"
            } else {
                "u32"
            };
            let Some(actual) = (match arg.ty.as_ref() {
                syn::Type::Path(path) => path.path.get_ident(),
                _ => None,
            }) else {
                return Err(syn::Error::new_spanned(
                    &arg.ty,
                    format!("builtin {argument_name} has type {expected}"),
                ));
            };
            if actual != expected {
                return Err(syn::Error::new_spanned(
                    &arg.ty,
                    format!("builtin {argument_name} has type {expected}"),
                ));
            }
            let allowed = match argument_name.as_str() {
                "vertex" | "instance" => stage == EntryStage::Vertex,
                _ => stage == EntryStage::Compute,
            };
            if !allowed {
                return Err(syn::Error::new_spanned(
                    &arg.ty,
                    format!("builtin {argument_name} belongs to another stage"),
                ));
            }
            builtins.push(input.clone());
            typecheck.push(input.clone());
            continue;
        }
        if let Some((output, element)) = varying_argument(&arg.ty) {
            if !output && stage != EntryStage::Fragment {
                return Err(syn::Error::new_spanned(
                    &arg.ty,
                    "only a fragment stage reads varyings",
                ));
            }
            if stage == EntryStage::Compute {
                return Err(syn::Error::new_spanned(
                    &arg.ty,
                    "a compute stage has no varyings",
                ));
            }
            let (element, position) = varying_element(&element, stage)?;
            let mut check = source;
            check.ty = Box::new(match element.as_str() {
                "fvec4" => syn::parse_quote!(::dome_shader::Fvec4),
                "fvec2" => syn::parse_quote!(::dome_shader::Fvec2),
                "f32" => syn::parse_quote!(f32),
                "u32" => syn::parse_quote!(u32),
                _ => unreachable!("a varying element is a shader scalar"),
            });
            if output {
                mark_typed_mutable(&mut check);
                outputs.push(argument_name.clone());
            }
            typecheck.push(FnArg::Typed(check));
            varyings.push(Varying {
                name: argument_name,
                element,
                output,
                position,
            });
            continue;
        }
        if let Some(kind) = resource_kind(&arg.ty) {
            if stage != EntryStage::Fragment {
                return Err(syn::Error::new_spanned(
                    &arg.ty,
                    "a texture or sampler belongs to the fragment stage",
                ));
            }
            if stage == EntryStage::Compute
                && matches!(
                    kind,
                    ResourceKind::TextureTable | ResourceKind::SamplerTable
                )
            {
                return Err(syn::Error::new_spanned(
                    &arg.ty,
                    "a compute kernel reads one texture or sampler",
                ));
            }
            resources.push(Resource {
                name: argument_name,
                kind,
            });
            let mut check = source;
            check.ty = Box::new(resource_type(kind));
            typecheck.push(FnArg::Typed(check));
            continue;
        }
        let access = storage_path(&arg.ty)
            .map(|segment| segment.ident.to_string())
            .ok_or_else(|| {
                syn::Error::new_spanned(
                    &arg.ty,
                    "kernel arguments are Read<T>, ReadWrite<T>, In<T> or Out<T>",
                )
            })?;
        let element = storage_element(&arg.ty)?;
        let record = match element.strip_suffix("Record") {
            Some(record) => {
                records.insert(
                    record.to_owned(),
                    syn::Ident::new(&element, proc_macro2::Span::call_site()),
                );
                true
            }
            None => false,
        };
        let element = element
            .strip_suffix("Record")
            .unwrap_or(&element)
            .to_owned();
        let mut check = source;
        if !record {
            let declared: syn::Type = if element == "AtomicU32" {
                syn::parse_quote!(::dome_shader::AtomicU32)
            } else {
                let declared = syn::Ident::new(&element, proc_macro2::Span::call_site());
                syn::parse_quote!(#declared)
            };
            let marker = if access == "ReadWrite" {
                syn::parse_quote!(::dome_shader::ReadWrite<#declared>)
            } else {
                syn::parse_quote!(::dome_shader::Read<#declared>)
            };
            check.ty = Box::new(marker);
        }
        if access == "ReadWrite" && writes(&source_function, variable.ident.clone()) {
            mark_typed_mutable(&mut check);
        }
        typecheck.push(FnArg::Typed(check));
        storages.push(Storage {
            name: argument_name,
            element,
            writable: access == "ReadWrite",
        });
    }
    if storages.is_empty() && stage == EntryStage::Compute {
        return Err(syn::Error::new_spanned(
            &entry.sig,
            "a compute kernel binds at least one storage buffer",
        ));
    }
    Ok(Bindings {
        builtins,
        typecheck,
        outputs,
        records,
        storages,
        resources,
        varyings,
    })
}

fn storage_declarations(storages: &[Storage], offset: u32) -> Vec<Tokens> {
    storages
        .iter()
        .enumerate()
        .map(|(slot, storage)| {
            let name = &storage.name;
            let element = storage
                .element
                .strip_suffix("Record")
                .unwrap_or(&storage.element);
            let slot = offset + slot as u32;
            let spec = if storage.writable {
                quote!(::dome_shader::BindingSpec::writable_storage(0, #slot))
            } else {
                quote!(::dome_shader::BindingSpec::storage(0, #slot))
            };
            quote!(compiler.storage_array(#name, #element, #spec);)
        })
        .collect()
}

fn resource_type(kind: ResourceKind) -> syn::Type {
    match kind {
        ResourceKind::Texture => syn::parse_quote!(::dome_shader::Texture2D),
        ResourceKind::Sampler => syn::parse_quote!(::dome_shader::Sampler),
        ResourceKind::TextureTable => syn::parse_quote!(::dome_shader::TextureTable),
        ResourceKind::SamplerTable => syn::parse_quote!(::dome_shader::SamplerTable),
    }
}

fn resource_declaration(name: &str, kind: ResourceKind, group: u32, slot: u32) -> Tokens {
    match kind {
        ResourceKind::Texture => {
            quote!(compiler.texture(#name, ::dome_shader::BindingSpec::sampled_image(#group, #slot));)
        }
        ResourceKind::Sampler => {
            quote!(compiler.sampler(#name, ::dome_shader::BindingSpec::sampler(#group, #slot));)
        }
        ResourceKind::TextureTable => {
            quote!(compiler.texture_table(#name, ::dome_shader::BindingSpec::sampled_image_table(#group, #slot, ::dome_shader::TEXTURE_TABLE_CAPACITY));)
        }
        ResourceKind::SamplerTable => {
            quote!(compiler.sampler_table(#name, ::dome_shader::BindingSpec::sampler_table(#group, #slot, ::dome_shader::SAMPLER_TABLE_CAPACITY));)
        }
    }
}

fn resource_declarations(resources: &[Resource]) -> Vec<Tokens> {
    resources
        .iter()
        .enumerate()
        .map(|(slot, resource)| resource_declaration(&resource.name, resource.kind, 1, slot as u32))
        .collect()
}

fn merge_storages(mut storages: Vec<Storage>, other: Vec<Storage>) -> syn::Result<Vec<Storage>> {
    for storage in other {
        match storages.iter().find(|present| present.name == storage.name) {
            Some(present)
                if present.element == storage.element && present.writable == storage.writable => {}
            Some(present) => {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "the graphics stages bind {} as different storage buffers",
                        present.name
                    ),
                ));
            }
            None => storages.push(storage),
        }
    }
    Ok(storages)
}

fn merge_resources(mut resources: Vec<Resource>, other: Vec<Resource>) -> Vec<Resource> {
    for resource in other {
        if !resources
            .iter()
            .any(|present| present.name == resource.name)
        {
            resources.push(resource);
        }
    }
    resources
}

fn varying_declarations(varyings: &[Varying]) -> Vec<Tokens> {
    let mut locations = [0u32; 2];
    varyings
        .iter()
        .map(|varying| {
            let name = &varying.name;
            let element = &varying.element;
            let space = if varying.output {
                quote!(::dome_shader::Space::Output)
            } else {
                quote!(::dome_shader::Space::Input)
            };
            let interface = if varying.position {
                quote!(::dome_shader::Interface::Position)
            } else {
                let direction = usize::from(varying.output);
                let location = locations[direction];
                locations[direction] += 1;
                quote!(::dome_shader::Interface::Location(#location))
            };
            quote!(compiler.varying(#name, #element, #space, #interface);)
        })
        .collect()
}

fn writes(function: &syn::ItemFn, name: syn::Ident) -> bool {
    let mut scan = Mentions {
        name: &name,
        present: false,
        written: false,
    };
    let mut scanned = function.clone();
    scan.visit_item_fn_mut(&mut scanned);
    scan.written
}

fn qualified_resources(function: &mut syn::ItemFn) {
    for input in &mut function.sig.inputs {
        let FnArg::Typed(argument) = input else {
            continue;
        };
        if let Some(kind) = resource_kind(&argument.ty) {
            *argument.ty = resource_type(kind);
        }
    }
}

fn assert_scalar_arguments(function: &syn::ItemFn) -> syn::Result<()> {
    for input in &function.sig.inputs {
        let FnArg::Typed(arg) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "shader helpers have no receiver",
            ));
        };
        if storage_path(&arg.ty).is_some() {
            return Err(syn::Error::new_spanned(
                &arg.ty,
                "only the entry reads storage buffers, and shader helpers take scalars",
            ));
        }
    }
    Ok(())
}

fn writable_arguments(function: &mut syn::ItemFn) {
    for input in &mut function.sig.inputs {
        let FnArg::Typed(arg) = input else {
            continue;
        };
        let Pat::Ident(variable) = arg.pat.as_mut() else {
            continue;
        };
        if storage_path(&arg.ty).is_some_and(|segment| segment.ident == "ReadWrite") {
            variable.mutability = Some(syn::token::Mut::default());
        }
    }
}

fn record_declarations(records: impl Iterator<Item = syn::Ident>) -> Vec<Tokens> {
    records
        .map(|record| {
            quote! {
                compiler.record(<#record as ::dome_shader::layout::Record>::LAYOUT);
            }
        })
        .collect()
}

fn expand_kernel_function(
    options: KernelOptions,
    mut original: syn::ItemFn,
) -> syn::Result<Tokens> {
    if !matches!(original.sig.output, syn::ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &original.sig.output,
            "compute entries do not return values",
        ));
    }
    let name = original.sig.ident.clone();
    let visibility = original.vis.clone();
    let entry = bindings(&mut original, EntryStage::Compute)?;
    let mut body = original.clone();
    body.sig.inputs = entry.builtins.clone();
    let definition = lower::function(&body)?;
    let typecheck = check_body(&original, &entry);
    let records = record_declarations(entry.records.values().cloned());
    let mut statements = storage_declarations(&entry.storages, 0);
    statements.extend(resource_declarations(&entry.resources));
    let size = options.size;
    Ok(quote! {
        #visibility fn #name() -> ::dome_shader::ComputeProgram {
            static PROGRAM: ::std::sync::OnceLock<::dome_shader::ComputeProgram> =
                ::std::sync::OnceLock::new();
            PROGRAM.get_or_init(|| {
                use ::dome_shader::ast as dome_ast;
                let mut compiler = ::dome_shader::Compiler::empty();
                compiler.constant("WORKGROUP_SIZE", #size);
                #(#records)*
                #(#statements)*
                compiler.function(#definition);
                compiler.finish(stringify!(#name), stringify!(#name), #size)
            }).clone()
        }
        const _: () = {
            use ::dome_shader::compiler::intrinsic::*;
            const WORKGROUP_SIZE: u32 = #size;
            #typecheck
            let _ = #name;
            let _ = WORKGROUP_SIZE;
        };
    })
}

fn expand_kernel_module(options: KernelOptions, module: syn::ItemMod) -> syn::Result<Tokens> {
    let module = shader_module(module)?;
    let name = module.name.clone();
    let visibility = module.visibility.clone();
    let Some(entry) = module.entry.clone() else {
        return Err(syn::Error::new_spanned(
            &module.functions[0].sig.ident,
            "a standalone kernel module marks one #[kernel] entry",
        ));
    };
    let declarations = shared_declarations(&module.shared);
    let mut definitions = Vec::new();
    let mut records = Vec::new();
    let mut statements = Vec::new();
    let mut checks = Vec::new();
    for function in &module.functions {
        let locals = shared_locals(function, &module.shared);
        if function.sig.ident == entry {
            let mut entry_function = function.clone();
            let entry_bindings = bindings(&mut entry_function, EntryStage::Compute)?;
            let mut body = entry_function;
            body.sig.inputs = entry_bindings.builtins.clone();
            definitions.push(lower::function(&body)?);
            records.extend(record_declarations(
                entry_bindings.records.values().cloned(),
            ));
            statements.extend(storage_declarations(&entry_bindings.storages, 0));
            statements.extend(resource_declarations(&entry_bindings.resources));
            let mut check = check_body(function, &entry_bindings);
            if !locals.is_empty() {
                check.block.stmts.splice(0..0, locals);
            }
            checks.push(quote!(#check));
            continue;
        }
        assert_scalar_arguments(function)?;
        definitions.push(lower::function(function)?);
        let mut check = function.clone();
        writable_arguments(&mut check);
        qualified_resources(&mut check);
        check.vis = syn::Visibility::Inherited;
        if !locals.is_empty() {
            check.block.stmts.splice(0..0, locals);
        }
        checks.push(quote!(#check));
    }
    let names = module.functions.iter().map(|function| &function.sig.ident);
    let size = options.size;
    Ok(quote! {
        #visibility fn #name() -> ::dome_shader::ComputeProgram {
            static PROGRAM: ::std::sync::OnceLock<::dome_shader::ComputeProgram> =
                ::std::sync::OnceLock::new();
            PROGRAM.get_or_init(|| {
                use ::dome_shader::ast as dome_ast;
                let mut compiler = ::dome_shader::Compiler::empty();
                compiler.constant("WORKGROUP_SIZE", #size);
                #(#declarations)*
                #(#records)*
                #(#statements)*
                #(compiler.function(#definitions);)*
                compiler.finish(stringify!(#name), stringify!(#entry), #size)
            }).clone()
        }
        const _: () = {
            use ::dome_shader::compiler::intrinsic::*;
            const WORKGROUP_SIZE: u32 = #size;
            #(#checks)*
            #(let _ = #names;)*
            let _ = WORKGROUP_SIZE;
        };
    })
}

fn is_stage_marker(attr: &syn::Attribute, stage: &str) -> bool {
    let segments = &attr.path().segments;
    segments.last().is_some_and(|part| part.ident == stage)
        && (segments.len() == 1 || segments.len() == 2 && segments[0].ident == "dome_shader")
}

enum SharedBinding {
    Storage(Storage),
    Resource(Resource),
}

struct GraphicsModule {
    name: syn::Ident,
    visibility: syn::Visibility,
    functions: Vec<syn::ItemFn>,
    bindings: Vec<SharedBinding>,
    vertex: syn::Ident,
    fragment: syn::Ident,
}

fn graphics_module(input: syn::ItemMod) -> syn::Result<GraphicsModule> {
    let Some((_, items)) = input.content else {
        return Err(syn::Error::new_spanned(
            input,
            "a graphics module is defined inline",
        ));
    };
    if let Some(attribute) = input.attrs.first() {
        return Err(syn::Error::new_spanned(
            attribute,
            "a graphics module carries no attribute of its own",
        ));
    }
    let visibility = input.vis;
    let name = input.ident;
    let mut functions = Vec::new();
    let mut bindings = Vec::new();
    let mut vertex = None;
    let mut fragment = None;
    for item in items {
        let syn::Item::Fn(mut function) = item else {
            return Err(syn::Error::new_spanned(
                item,
                "a graphics module contains Rust functions",
            ));
        };
        let mut marked = None;
        let mut binding = false;
        for attr in &function.attrs {
            if is_stage_marker(attr, "binding") {
                if binding || marked.is_some() || !matches!(attr.meta, syn::Meta::Path(_)) {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "a graphics binding is the only marker of its function",
                    ));
                }
                binding = true;
                continue;
            }
            let stage = if is_stage_marker(attr, "vertex") {
                EntryStage::Vertex
            } else if is_stage_marker(attr, "fragment") {
                EntryStage::Fragment
            } else {
                return Err(syn::Error::new_spanned(
                    attr,
                    "graphics module functions only use #[binding], #[vertex] or #[fragment]",
                ));
            };
            if !matches!(attr.meta, syn::Meta::Path(_)) || marked.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "a graphics module marks one vertex and one fragment entry",
                ));
            }
            marked = Some(stage);
        }
        function.attrs.clear();
        if binding {
            if marked.is_some() {
                return Err(syn::Error::new_spanned(
                    &function.sig,
                    "a graphics binding declares no stage of its own",
                ));
            }
            if !matches!(function.sig.output, syn::ReturnType::Default)
                || !function.block.stmts.is_empty()
            {
                return Err(syn::Error::new_spanned(
                    &function.sig,
                    "a graphics binding declares no body or result",
                ));
            }
            if function.sig.inputs.is_empty() {
                return Err(syn::Error::new_spanned(
                    &function.sig,
                    "a graphics binding declares one resource",
                ));
            }
            for argument in &function.sig.inputs {
                let FnArg::Typed(argument) = argument else {
                    return Err(syn::Error::new_spanned(
                        argument,
                        "a graphics binding is named",
                    ));
                };
                let Pat::Ident(variable) = argument.pat.as_ref() else {
                    return Err(syn::Error::new_spanned(
                        &argument.pat,
                        "a graphics binding is named",
                    ));
                };
                if let Some(kind) = resource_kind(&argument.ty) {
                    bindings.push(SharedBinding::Resource(Resource {
                        name: variable.ident.to_string(),
                        kind,
                    }));
                    continue;
                }
                let access = storage_path(&argument.ty)
                    .map(|segment| segment.ident.to_string())
                    .ok_or_else(|| {
                        syn::Error::new_spanned(
                            &argument.ty,
                            "a graphics binding is Read<T>, ReadWrite<T>, Texture2D or Sampler",
                        )
                    })?;
                let element = storage_element(&argument.ty)?;
                bindings.push(SharedBinding::Storage(Storage {
                    name: variable.ident.to_string(),
                    element,
                    writable: access == "ReadWrite",
                }));
            }
            continue;
        }
        if let Some(stage) = marked {
            let slot = match stage {
                EntryStage::Vertex => &mut vertex,
                _ => &mut fragment,
            };
            if slot.is_some() {
                return Err(syn::Error::new_spanned(
                    &function.sig.ident,
                    "a graphics module marks one vertex and one fragment entry",
                ));
            }
            *slot = Some(function.sig.ident.clone());
        }
        functions.push(function);
    }
    let (Some(vertex), Some(fragment)) = (vertex, fragment) else {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "a graphics module marks one vertex and one fragment entry",
        ));
    };
    if functions.len() < 2 {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "a graphics module needs a vertex and a fragment function",
        ));
    }
    Ok(GraphicsModule {
        name,
        visibility,
        functions,
        bindings,
        vertex,
        fragment,
    })
}

fn expand_graphics(module: GraphicsModule) -> syn::Result<Tokens> {
    let name = module.name.clone();
    let visibility = module.visibility.clone();
    let vertex = module.vertex.clone();
    let fragment = module.fragment.clone();
    let mut definitions = Vec::new();
    let mut records = BTreeMap::new();
    let shared = module.bindings;
    for binding in &shared {
        if let SharedBinding::Storage(storage) = binding {
            let Some(record) = storage.element.strip_suffix("Record") else {
                continue;
            };
            records.insert(
                record.to_owned(),
                syn::Ident::new(&storage.element, proc_macro2::Span::call_site()),
            );
        }
    }
    let local_bindings = shared
        .iter()
        .map(|binding| {
            let statement = match binding {
                SharedBinding::Storage(storage) => {
                    let name = syn::Ident::new(&storage.name, proc_macro2::Span::call_site());
                    let element =
                        syn::Ident::new(&storage.element, proc_macro2::Span::call_site());
                    if storage.writable {
                        quote!(let mut #name: ::dome_shader::ReadWrite<#element> = ::core::default::Default::default();)
                    } else {
                        quote!(let #name: ::dome_shader::Read<#element> = ::core::default::Default::default();)
                    }
                }
                SharedBinding::Resource(resource) => {
                    let name = syn::Ident::new(&resource.name, proc_macro2::Span::call_site());
                    let ty = resource_type(resource.kind);
                    quote!(let #name: #ty = ::core::default::Default::default();)
                }
            };
            syn::parse2::<syn::Stmt>(statement)
                .expect("a shared graphics binding is a shader local")
        })
        .collect::<Vec<_>>();
    let mut storages: Vec<Storage> = Vec::new();
    let mut resources: Vec<Resource> = Vec::new();
    let mut vertex_varyings: Vec<Varying> = Vec::new();
    let mut fragment_varyings: Vec<Varying> = Vec::new();
    let mut checks = Vec::new();
    for function in &module.functions {
        let stage = if function.sig.ident == vertex {
            Some(EntryStage::Vertex)
        } else if function.sig.ident == fragment {
            Some(EntryStage::Fragment)
        } else {
            None
        };
        let Some(stage) = stage else {
            assert_scalar_arguments(function)?;
            definitions.push(lower::function(function)?);
            let mut check = function.clone();
            writable_arguments(&mut check);
            qualified_resources(&mut check);
            check.vis = syn::Visibility::Inherited;
            checks.push(quote!(#check));
            continue;
        };
        let mut entry_function = function.clone();
        let bindings = bindings(&mut entry_function, stage)?;
        let mut body = entry_function;
        body.sig.inputs = bindings.builtins.clone();
        definitions.push(lower::function(&body)?);
        checks.push({
            let mut check = check_body(function, &bindings);
            check
                .block
                .stmts
                .splice(0..0, local_bindings.iter().cloned());
            quote!(#check)
        });
        records.extend(bindings.records);
        storages = merge_storages(storages, bindings.storages)?;
        resources = merge_resources(resources, bindings.resources);
        match stage {
            EntryStage::Vertex => vertex_varyings.extend(bindings.varyings),
            _ => fragment_varyings.extend(bindings.varyings),
        }
    }
    let records = record_declarations(records.values().cloned());
    let shared_storages = shared
        .iter()
        .filter(|binding| matches!(binding, SharedBinding::Storage(_)))
        .count() as u32;
    let mut shared_storage_slot: u32 = 0;
    let mut shared_resource_slot: u32 = 0;
    let mut shared_storage_bindings = Vec::new();
    let mut shared_resource_bindings = Vec::new();
    for binding in &shared {
        match binding {
            SharedBinding::Storage(storage) => {
                let name = &storage.name;
                let element = storage
                    .element
                    .strip_suffix("Record")
                    .unwrap_or(&storage.element);
                let slot = shared_storage_slot;
                shared_storage_slot += 1;
                shared_storage_bindings.push(if storage.writable {
                    quote!(compiler.storage_array(#name, #element, ::dome_shader::BindingSpec::writable_storage(0, #slot));)
                } else {
                    quote!(compiler.storage_array(#name, #element, ::dome_shader::BindingSpec::storage(0, #slot));)
                });
            }
            SharedBinding::Resource(resource) => {
                let name = &resource.name;
                let slot = shared_resource_slot;
                shared_resource_slot += 1;
                shared_resource_bindings.push(resource_declaration(name, resource.kind, 2, slot));
            }
        }
    }
    let mut storages = storage_declarations(&storages, shared_storages);
    storages.extend(resource_declarations(&resources));
    let vertex_varyings = varying_declarations(&vertex_varyings);
    let fragment_varyings = varying_declarations(&fragment_varyings);
    let names = module.functions.iter().map(|function| &function.sig.ident);
    Ok(quote! {
        #visibility fn #name() -> ::dome_shader::GraphicsProgram {
            static PROGRAM: ::std::sync::OnceLock<::dome_shader::GraphicsProgram> =
                ::std::sync::OnceLock::new();
            PROGRAM.get_or_init(|| {
                use ::dome_shader::ast as dome_ast;
                let mut compiler = ::dome_shader::Compiler::empty();
                #(#records)*
                #(#shared_storage_bindings)*
                #(#storages)*
                #(#shared_resource_bindings)*
                #(#vertex_varyings)*
                #(#fragment_varyings)*
                #(compiler.function(#definitions);)*
                compiler.finish_graphics(
                    stringify!(#name),
                    stringify!(#vertex),
                    stringify!(#fragment),
                )
            }).clone()
        }
        const _: () = {
            use ::dome_shader::compiler::intrinsic::*;
            #(#checks)*
            #(let _ = #names;)*
            let _ = #name;
        };
    })
}

#[proc_macro_attribute]
pub fn graphics(attr: TokenStream, item: TokenStream) -> TokenStream {
    let result = if attr.is_empty() {
        syn::parse::<syn::ItemMod>(item)
            .and_then(graphics_module)
            .and_then(expand_graphics)
    } else {
        Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "a graphics module has no attributes",
        ))
    };
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    let result = if attr.is_empty() {
        syn::parse::<syn::ItemMod>(item)
            .and_then(shader_module)
            .and_then(expand_definitions)
    } else {
        Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "kernel modules have no attributes",
        ))
    };
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

#[proc_macro]
pub fn expression(input: TokenStream) -> TokenStream {
    let result =
        syn::parse::<syn::Expr>(input).and_then(|expression| lower::expression(&expression));
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

#[proc_macro_attribute]
pub fn kernel(attr: TokenStream, item: TokenStream) -> TokenStream {
    let result = syn::parse::<KernelOptions>(attr).and_then(|options| {
        syn::parse::<syn::Item>(item).and_then(|item| match item {
            syn::Item::Fn(function) => expand_kernel_function(options, function),
            syn::Item::Mod(module) => expand_kernel_module(options, module),
            other => Err(syn::Error::new_spanned(
                other,
                "a kernel is a function or a module of functions",
            )),
        })
    });
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}
