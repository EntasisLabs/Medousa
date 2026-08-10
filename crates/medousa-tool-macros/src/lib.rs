//! Attribute macro for stateful, typed Stasis tools.
//!
//! The generated code deliberately targets a configurable support path. This
//! crate parses signatures and emits generic adapter mechanics; it does not
//! depend on Medousa runtime policy.

use proc_macro::TokenStream;

use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Expr, ExprLit, FnArg, GenericArgument, ImplItem, ImplItemFn, ItemImpl, Lit, LitStr,
    Meta, MetaNameValue, Path, PathArguments, ReturnType, Type, parse_quote,
};

#[proc_macro_attribute]
pub fn medousa_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_medousa_tool(attr, item) {
        Ok(expanded) => expanded,
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_medousa_tool(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let arguments = Punctuated::<MetaNameValue, syn::Token![,]>::parse_terminated.parse(attr)?;
    let (id, support_path) = parse_arguments(arguments)?;
    let item_impl = syn::parse::<ItemImpl>(item)?;

    validate_impl(&item_impl)?;
    let handler = find_handler(&item_impl)?;
    let (input_type, output_type) = validate_handler(handler)?;
    let description = handler_description(handler)?;
    let self_type = item_impl.self_ty.as_ref();

    let expanded = quote! {
        #item_impl

        impl #support_path::TypedTool for #self_type {
            type Input = #input_type;
            type Output = #output_type;

            fn tool_id() -> #support_path::ToolId {
                #support_path::resolve_tool_id(#id)
            }

            fn description() -> &'static str {
                #description
            }

            fn contract() -> &'static #support_path::ToolContract {
                static CONTRACT: #support_path::__private::OnceLock<#support_path::ToolContract> =
                    #support_path::__private::OnceLock::new();

                CONTRACT.get_or_init(|| {
                    #support_path::build_contract::<Self>().unwrap_or_else(|error| {
                        panic!("invalid typed tool contract: {error}")
                    })
                })
            }
        }

        #[#support_path::__private::async_trait::async_trait]
        impl #support_path::__private::StasisTool for #self_type {
            fn name(&self) -> &'static str {
                <Self as #support_path::TypedTool>::tool_id().as_str()
            }

            fn description(&self) -> ::core::option::Option<&'static str> {
                ::core::option::Option::Some(
                    <Self as #support_path::TypedTool>::description()
                )
            }

            fn input_schema(
                &self,
            ) -> ::core::option::Option<#support_path::__private::Value> {
                ::core::option::Option::Some(
                    <Self as #support_path::TypedTool>::contract().input_schema.clone()
                )
            }

            fn output_schema(
                &self,
            ) -> ::core::option::Option<#support_path::__private::Value> {
                ::core::option::Option::Some(
                    <Self as #support_path::TypedTool>::contract().output_schema.clone()
                )
            }

            async fn invoke(
                &self,
                input: #support_path::__private::Value,
            ) -> #support_path::__private::StasisResult<#support_path::__private::Value> {
                let tool_id = <Self as #support_path::TypedTool>::tool_id();
                let typed_input = #support_path::deserialize_input::<
                    <Self as #support_path::TypedTool>::Input
                >(tool_id, input)?;
                let typed_output: <Self as #support_path::TypedTool>::Output =
                    self.invoke_typed(typed_input).await?;

                #support_path::serialize_output(tool_id, typed_output)
            }
        }
    };

    Ok(expanded.into())
}

fn parse_arguments(
    arguments: Punctuated<MetaNameValue, syn::Token![,]>,
) -> syn::Result<(Expr, Path)> {
    let mut id = None;
    let mut support_path = None;

    for argument in arguments {
        if argument.path.is_ident("id") {
            if id.is_some() {
                return Err(syn::Error::new_spanned(argument, "duplicate `id` argument"));
            }
            match &argument.value {
                Expr::Path(_)
                | Expr::Lit(ExprLit {
                    lit: Lit::Str(_), ..
                }) => {
                    id = Some(argument.value);
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        argument.value,
                        "`id` must be a typed constant, static string constant, or string literal",
                    ));
                }
            }
            continue;
        }

        if argument.path.is_ident("crate_path") {
            if support_path.is_some() {
                return Err(syn::Error::new_spanned(
                    argument,
                    "duplicate `crate_path` argument",
                ));
            }
            support_path = Some(parse_support_path(argument.value)?);
            continue;
        }

        return Err(syn::Error::new_spanned(
            argument.path,
            "unsupported argument; expected `id` or `crate_path`",
        ));
    }

    let id = id.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "missing required argument: `id = TOOL_ID`",
        )
    })?;

    Ok((
        id,
        support_path.unwrap_or_else(|| parse_quote!(crate::typed_tools)),
    ))
}

fn parse_support_path(expression: Expr) -> syn::Result<Path> {
    match expression {
        Expr::Path(path) => Ok(path.path),
        Expr::Lit(ExprLit {
            lit: Lit::Str(path),
            ..
        }) => syn::parse_str(&path.value()).map_err(|error| {
            syn::Error::new(path.span(), format!("invalid `crate_path`: {error}"))
        }),
        other => Err(syn::Error::new_spanned(
            other,
            "`crate_path` must be a Rust path or string literal containing one",
        )),
    }
}

fn validate_impl(item_impl: &ItemImpl) -> syn::Result<()> {
    if item_impl.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            item_impl,
            "`medousa_tool` must annotate an inherent impl, not a trait impl",
        ));
    }

    if !item_impl.generics.params.is_empty() || item_impl.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &item_impl.generics,
            "`medousa_tool` does not support generic impls",
        ));
    }

    let Type::Path(type_path) = item_impl.self_ty.as_ref() else {
        return Err(syn::Error::new_spanned(
            &item_impl.self_ty,
            "`medousa_tool` requires a concrete tool type",
        ));
    };

    if type_path.qself.is_some()
        || type_path
            .path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, PathArguments::None))
    {
        return Err(syn::Error::new_spanned(
            &item_impl.self_ty,
            "`medousa_tool` requires a concrete, non-generic tool type",
        ));
    }

    Ok(())
}

fn find_handler(item_impl: &ItemImpl) -> syn::Result<&ImplItemFn> {
    let handlers = item_impl
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) if method.sig.ident == "invoke_typed" => Some(method),
            _ => None,
        })
        .collect::<Vec<_>>();

    match handlers.as_slice() {
        [handler] => Ok(handler),
        [] => Err(syn::Error::new_spanned(
            item_impl,
            "`medousa_tool` impl must contain exactly one `invoke_typed` method",
        )),
        _ => Err(syn::Error::new_spanned(
            &handlers[1].sig.ident,
            "`medousa_tool` impl contains more than one `invoke_typed` method",
        )),
    }
}

fn validate_handler(handler: &ImplItemFn) -> syn::Result<(Type, Type)> {
    if handler.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            handler.sig.fn_token,
            "`invoke_typed` must be async",
        ));
    }

    if !handler.sig.generics.params.is_empty() || handler.sig.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &handler.sig.generics,
            "`invoke_typed` does not support generics",
        ));
    }

    if handler.sig.inputs.len() != 2 {
        return Err(syn::Error::new_spanned(
            &handler.sig.inputs,
            "`invoke_typed` must accept exactly `&self` and one typed input",
        ));
    }

    let receiver = match handler.sig.inputs.first() {
        Some(FnArg::Receiver(receiver)) => receiver,
        Some(other) => {
            return Err(syn::Error::new_spanned(
                other,
                "the first `invoke_typed` argument must be `&self`",
            ));
        }
        None => unreachable!(),
    };

    if receiver.reference.is_none()
        || receiver.mutability.is_some()
        || receiver.colon_token.is_some()
    {
        return Err(syn::Error::new_spanned(
            receiver,
            "`invoke_typed` must use an immutable `&self` receiver",
        ));
    }

    let input_type = match handler.sig.inputs.iter().nth(1) {
        Some(FnArg::Typed(input)) => input.ty.as_ref().clone(),
        Some(FnArg::Receiver(receiver)) => {
            return Err(syn::Error::new_spanned(
                receiver,
                "the second `invoke_typed` argument must be a typed input",
            ));
        }
        None => unreachable!(),
    };

    let output_type = result_output_type(&handler.sig.output)?;
    Ok((input_type, output_type))
}

fn result_output_type(output: &ReturnType) -> syn::Result<Type> {
    let ReturnType::Type(_, return_type) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "`invoke_typed` must return `Result<Output>`",
        ));
    };
    let Type::Path(result_path) = return_type.as_ref() else {
        return Err(syn::Error::new_spanned(
            return_type,
            "`invoke_typed` must return `Result<Output>`",
        ));
    };
    let Some(result_segment) = result_path.path.segments.last() else {
        return Err(syn::Error::new_spanned(
            return_type,
            "`invoke_typed` must return `Result<Output>`",
        ));
    };

    if result_segment.ident != "Result" {
        return Err(syn::Error::new_spanned(
            return_type,
            "`invoke_typed` must return `Result<Output>`",
        ));
    }

    let PathArguments::AngleBracketed(arguments) = &result_segment.arguments else {
        return Err(syn::Error::new_spanned(
            &result_segment.arguments,
            "`Result` must declare an output type",
        ));
    };

    arguments
        .args
        .iter()
        .find_map(|argument| match argument {
            GenericArgument::Type(output_type) => Some(output_type.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            syn::Error::new_spanned(&arguments.args, "`Result` must declare an output type")
        })
}

fn handler_description(handler: &ImplItemFn) -> syn::Result<LitStr> {
    let mut lines = Vec::new();
    for attribute in &handler.attrs {
        if let Some(line) = doc_line(attribute)? {
            lines.push(line);
        }
    }

    let description = lines.join("\n");
    let description = description.trim();
    if description.is_empty() {
        return Err(syn::Error::new_spanned(
            &handler.sig.ident,
            "`invoke_typed` requires a doc comment used as the tool description",
        ));
    }

    Ok(LitStr::new(description, handler.sig.ident.span()))
}

fn doc_line(attribute: &Attribute) -> syn::Result<Option<String>> {
    if !attribute.path().is_ident("doc") {
        return Ok(None);
    }

    let Meta::NameValue(name_value) = &attribute.meta else {
        return Err(syn::Error::new_spanned(
            attribute,
            "tool documentation must use ordinary doc comments",
        ));
    };
    let Expr::Lit(ExprLit {
        lit: Lit::Str(line),
        ..
    }) = &name_value.value
    else {
        return Err(syn::Error::new_spanned(
            attribute,
            "tool documentation must be a string",
        ));
    };

    Ok(Some(line.value().trim().to_string()))
}
