use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{ItemStruct, parse_macro_input};

fn validate_export(attribute: TokenStream2, item: &ItemStruct) -> syn::Result<()> {
    if !attribute.is_empty() {
        return Err(syn::Error::new_spanned(
            attribute,
            "#[extension] takes no arguments; declare identity and capabilities in Extension::metadata()",
        ));
    }
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "#[extension] requires a concrete struct without generic parameters",
        ));
    }
    Ok(())
}

fn sdk_path() -> syn::Result<TokenStream2> {
    match crate_name("atri_extensions") {
        // The SDK declares this alias itself; it also works for its example targets.
        Ok(FoundCrate::Itself) => Ok(quote!(::atri_extensions)),
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{name}");
            Ok(quote!(::#name))
        }
        Err(error) => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("#[extension] requires an atri_extensions dependency: {error}"),
        )),
    }
}

fn export(item: &ItemStruct, sdk: TokenStream2) -> TokenStream2 {
    let name = &item.ident;
    quote! {
        #item
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub extern "C" fn atri_extension_v1() -> *const #sdk::abi::NativeExtensionV1 {
            static API: #sdk::abi::NativeExtensionV1 = #sdk::native::exports::<#name>();
            &API
        }
        #[cfg(target_arch = "wasm32")]
        #sdk::__export_guest_extension!(#name);
    }
}

/// Export a concrete extension struct for native and feature-selected Wasm backends.
///
/// Use this once per artifact. Implement `Extension::metadata()` to declare all
/// identity, role, services, subscriptions, configuration, and capabilities.
#[proc_macro_attribute]
pub fn extension(attribute: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemStruct);
    let expansion = validate_export(attribute.into(), &item)
        .and_then(|()| sdk_path())
        .map(|sdk| export(&item, sdk));
    expansion
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_obsolete_identity_and_all_other_arguments() {
        let item = syn::parse_quote!(
            struct Probe;
        );
        for arguments in [quote!(id = "abc"), quote!("abc"), quote!(startup = "other")] {
            assert!(validate_export(arguments, &item).is_err());
        }
    }

    #[test]
    fn rejects_generic_exported_structs() {
        for item in [
            syn::parse_quote!(
                struct Probe<T>(T);
            ),
            syn::parse_quote!(
                struct Probe<'a>(&'a str);
            ),
            syn::parse_quote!(
                struct Probe<const N: usize>([u8; N]);
            ),
        ] {
            assert!(validate_export(TokenStream2::new(), &item).is_err());
        }
    }
}
