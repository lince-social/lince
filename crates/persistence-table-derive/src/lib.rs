use proc_macro::TokenStream;
use quote::quote;
use syn::ext::IdentExt;
use syn::{
    Data, DeriveInput, Fields, LitStr, Token, Type, parse::Parse, parse::ParseStream,
    parse_macro_input,
};

#[proc_macro_derive(Table, attributes(table))]
pub fn derive_table(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_table(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_table(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = input.ident;
    let struct_attrs = parse_struct_attrs(&input.attrs)?;
    let fields = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    ident,
                    "Table derive requires a struct with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                ident,
                "Table derive can only be used on structs",
            ));
        }
    };

    let table_name = struct_attrs
        .name
        .as_deref()
        .ok_or_else(|| syn::Error::new_spanned(&ident, "missing #[table(name = \"...\")]"))?;

    let mut column_defs = Vec::new();
    for field in fields {
        let field_ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new_spanned(&ident, "field ident missing"))?;
        let field_name = field_ident.to_string();
        let field_attrs = parse_field_attrs(&field.attrs)?;
        let (nullable, inner_type) = option_inner_type(&field.ty);
        let sql_type = if let Some(sql_type) = field_attrs.sql_type.as_deref() {
            sql_type.to_string()
        } else {
            infer_sql_type(inner_type)?
        };
        let primary_key = field_attrs.primary_key;
        let unique = field_attrs.unique;
        let default_sql = field_attrs
            .default
            .as_ref()
            .map(|value| quote! { Some(#value) })
            .unwrap_or_else(|| quote! { None });
        let references_sql = field_attrs
            .references
            .as_ref()
            .map(|value| quote! { Some(#value) })
            .unwrap_or_else(|| quote! { None });
        let check_sql = field_attrs
            .check
            .as_ref()
            .map(|value| quote! { Some(#value) })
            .unwrap_or_else(|| quote! { None });

        column_defs.push(quote! {
            crate::schema::types::ColumnDef {
                name: #field_name,
                sql_type: #sql_type,
                nullable: #nullable,
                primary_key: #primary_key,
                unique: #unique,
                default_sql: #default_sql,
                references_sql: #references_sql,
                check_sql: #check_sql,
            }
        });
    }

    let index_defs = struct_attrs.indexes.iter().map(|index| {
        let name = index.name.as_str();
        let columns = index.columns.iter().map(String::as_str).collect::<Vec<_>>();
        let unique = index.unique;
        let where_sql = index
            .where_sql
            .as_ref()
            .map(|value| quote! { Some(#value) })
            .unwrap_or_else(|| quote! { None });
        quote! {
            crate::schema::types::IndexDef {
                name: #name,
                columns: vec![#(#columns),*],
                unique: #unique,
                where_sql: #where_sql,
            }
        }
    });

    let checks = struct_attrs
        .checks
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let composite_primary_key = struct_attrs.composite_primary_key.as_ref().map(|columns| {
        let columns = columns.iter().map(String::as_str).collect::<Vec<_>>();
        quote! { Some(vec![#(#columns),*]) }
    });

    let strict = struct_attrs.strict;
    let composite_primary_key = composite_primary_key.unwrap_or_else(|| quote! { None });

    Ok(quote! {
        impl crate::schema::types::Table for #ident {
            fn schema() -> crate::schema::types::TableSchema {
                crate::schema::types::TableSchema {
                    name: #table_name,
                    strict: #strict,
                    columns: vec![#(#column_defs),*],
                    indexes: vec![#(#index_defs),*],
                    checks: vec![#(#checks),*],
                    composite_primary_key: #composite_primary_key,
                }
            }
        }
    })
}

#[derive(Default)]
struct StructAttrs {
    name: Option<String>,
    strict: bool,
    checks: Vec<String>,
    indexes: Vec<IndexAttr>,
    composite_primary_key: Option<Vec<String>>,
}

#[derive(Default)]
struct FieldAttrs {
    primary_key: bool,
    unique: bool,
    default: Option<String>,
    sql_type: Option<String>,
    references: Option<String>,
    check: Option<String>,
}

#[derive(Default)]
struct IndexAttr {
    name: String,
    columns: Vec<String>,
    unique: bool,
    where_sql: Option<String>,
}

fn parse_struct_attrs(attrs: &[syn::Attribute]) -> syn::Result<StructAttrs> {
    let mut parsed = StructAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("table") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                parsed.name = Some(meta.value()?.parse::<LitStr>()?.value());
                return Ok(());
            }
            if meta.path.is_ident("strict") {
                parsed.strict = true;
                return Ok(());
            }
            if meta.path.is_ident("check") {
                parsed.checks.push(meta.value()?.parse::<LitStr>()?.value());
                return Ok(());
            }
            if meta.path.is_ident("index") {
                let content;
                syn::parenthesized!(content in meta.input);
                parsed.indexes.push(content.parse::<IndexAttr>()?);
                return Ok(());
            }
            if meta.path.is_ident("primary_key") {
                let content;
                syn::parenthesized!(content in meta.input);
                let composite = content.parse::<CompositePrimaryKeyAttr>()?;
                parsed.composite_primary_key = Some(composite.columns);
                return Ok(());
            }

            Err(meta.error("unsupported #[table(...)] attribute"))
        })?;
    }

    Ok(parsed)
}

fn parse_field_attrs(attrs: &[syn::Attribute]) -> syn::Result<FieldAttrs> {
    let mut parsed = FieldAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("table") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("primary_key") {
                parsed.primary_key = true;
                return Ok(());
            }
            if meta.path.is_ident("unique") {
                parsed.unique = true;
                return Ok(());
            }
            if meta.path.is_ident("default") {
                parsed.default = Some(meta.value()?.parse::<LitStr>()?.value());
                return Ok(());
            }
            if meta.path.is_ident("sql_type") {
                parsed.sql_type = Some(meta.value()?.parse::<LitStr>()?.value());
                return Ok(());
            }
            if meta.path.is_ident("references") {
                parsed.references = Some(meta.value()?.parse::<LitStr>()?.value());
                return Ok(());
            }
            if meta.path.is_ident("check") {
                parsed.check = Some(meta.value()?.parse::<LitStr>()?.value());
                return Ok(());
            }

            Err(meta.error("unsupported field #[table(...)] attribute"))
        })?;
    }

    Ok(parsed)
}

fn option_inner_type(ty: &Type) -> (bool, &Type) {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Option"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
    {
        return (true, inner);
    }

    (false, ty)
}

fn infer_sql_type(ty: &Type) -> syn::Result<String> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        let ident = segment.ident.to_string();
        let sql_type = match ident.as_str() {
            "String" => "TEXT",
            "bool" | "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "usize" => "INTEGER",
            "f32" | "f64" => "REAL",
            _ => {
                return Err(syn::Error::new_spanned(
                    ty,
                    "cannot infer SQLite type for this field; use #[table(sql_type = \"...\")]",
                ));
            }
        };
        return Ok(sql_type.to_string());
    }

    Err(syn::Error::new_spanned(
        ty,
        "unsupported field type for Table derive",
    ))
}

struct CompositePrimaryKeyAttr {
    columns: Vec<String>,
}

impl Parse for CompositePrimaryKeyAttr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut columns = None;
        while !input.is_empty() {
            let ident = input.call(syn::Ident::parse_any)?;
            input.parse::<Token![=]>()?;
            if ident == "columns" {
                columns = Some(parse_csv_lit(input.parse::<LitStr>()?.value()));
            } else {
                return Err(syn::Error::new_spanned(
                    ident,
                    "unsupported primary_key option",
                ));
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            columns: columns
                .ok_or_else(|| input.error("primary_key requires columns = \"...\""))?,
        })
    }
}

impl Parse for IndexAttr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut name = None;
        let mut columns = None;
        let mut unique = false;
        let mut where_sql = None;

        while !input.is_empty() {
            let ident = input.call(syn::Ident::parse_any)?;
            if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
                if ident == "name" {
                    name = Some(input.parse::<LitStr>()?.value());
                } else if ident == "columns" {
                    columns = Some(parse_csv_lit(input.parse::<LitStr>()?.value()));
                } else if ident == "where" {
                    where_sql = Some(input.parse::<LitStr>()?.value());
                } else {
                    return Err(syn::Error::new_spanned(ident, "unsupported index option"));
                }
            } else if ident == "unique" {
                unique = true;
            } else {
                return Err(syn::Error::new_spanned(ident, "unsupported index flag"));
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            name: name.ok_or_else(|| input.error("index requires name = \"...\""))?,
            columns: columns.ok_or_else(|| input.error("index requires columns = \"...\""))?,
            unique,
            where_sql,
        })
    }
}

fn parse_csv_lit(raw: String) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}
