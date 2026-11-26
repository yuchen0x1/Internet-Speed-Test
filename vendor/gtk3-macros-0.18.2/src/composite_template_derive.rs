// Take a look at the license at the top of the repository in the LICENSE file.

use proc_macro2::TokenStream;
use quote::quote;
use syn::Data;

use std::string::ToString;

use crate::attribute_parser::*;
use crate::util::*;

fn gen_set_template(source: TemplateSource) -> TokenStream {
    match source {
        TemplateSource::File(file) => quote! {
            let t = include_bytes!(#file);
            klass.set_template(t);
        },
        TemplateSource::Resource(resource) => quote! {
            klass.set_template_from_resource(&#resource);
        },
        TemplateSource::String(template) => quote! {
            klass.set_template(&#template);
        },
    }
}

fn gen_template_child_bindings(fields: &syn::Fields) -> syn::Result<TokenStream> {
    let crate_ident = crate_ident_new();
    let attributed_fields = parse_fields(fields)?;

    let recurse = attributed_fields.iter().map(|field| match field.attr.ty {
        FieldAttributeType::TemplateChild => {
            let mut value_id = &field.ident.to_string();
            let ident = &field.ident;
            field.attr.args.iter().for_each(|arg| match arg {
                FieldAttributeArg::Id(value) => {
                    value_id = value;
                }
            });

            quote! {
                klass.bind_template_child_with_offset(
                    &#value_id,
                    #crate_ident::offset_of!(Self => #ident),
                );
            }
        }
    });

    Ok(quote! {
        #(#recurse)*
    })
}

pub fn impl_composite_template(input: &syn::DeriveInput) -> TokenStream {
    let name = &input.ident;
    let crate_ident = crate_ident_new();

    let source = match parse_template_source(input) {
        Ok(v) => v,
        Err(e) => return syn::Error::new_spanned(
            input,
            format!("{e}: derive(CompositeTemplate) requires #[template(...)] to specify 'file', 'resource', or 'string'"),
        ).to_compile_error(),
    };

    let set_template = gen_set_template(source);

    let fields = match input.data {
        Data::Struct(ref s) => &s.fields,
        _ => {
            return syn::Error::new_spanned(
                input,
                "derive(CompositeTemplate) only supports structs",
            )
            .to_compile_error()
        }
    };

    let template_children = match gen_template_child_bindings(fields) {
        Ok(children) => children,
        Err(error) => return error.to_compile_error(),
    };

    quote! {
        impl #crate_ident::subclass::widget::CompositeTemplate for #name {
            fn bind_template(klass: &mut Self::Class) {
                #set_template

                unsafe {
                    #template_children
                }
            }
        }
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::impl_composite_template;

    #[test]
    fn missing_template_returns_compile_error_tokens() {
        let input = syn::parse_quote!(
            struct Example {}
        );
        let diagnostics = impl_composite_template(&input).to_string();
        assert!(diagnostics.contains("compile_error"));
        assert!(diagnostics.contains("requires #[template(...)]"));
    }

    #[test]
    fn malformed_template_child_returns_compile_error_tokens() {
        let input = syn::parse_quote!(
            #[template(string = "<interface />")]
            struct Example {
                #[template_child(id = "first", id = "second")]
                child: (),
            }
        );
        let diagnostics = impl_composite_template(&input).to_string();
        assert!(diagnostics.contains("compile_error"));
        assert!(diagnostics.contains("two instances of the same attribute"));
    }
}
