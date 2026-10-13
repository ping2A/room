//! Proc-macros for the Room WASM guest SDK (`docs/12` §11).
//!
//! Full `#[derive(Actor)]` / `states!` land later; this crate seeds the surface.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Item, parse_macro_input};

/// Placeholder attribute: keeps the item (full Actor derive is later).
#[proc_macro_attribute]
pub fn actor(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Attach a no-op `__room_register() -> i32` for `export_module!(classes = [...])`.
#[proc_macro_attribute]
pub fn register_stub(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as Item);
    let name = match &item {
        Item::Struct(s) => &s.ident,
        Item::Enum(e) => &e.ident,
        Item::Union(u) => &u.ident,
        _ => {
            return syn::Error::new_spanned(item, "register_stub expects struct/enum/union")
                .to_compile_error()
                .into();
        }
    };
    quote! {
        #item
        impl #name {
            /// Registration hook for `export_module!(classes = [...])`.
            pub fn __room_register() -> i32 {
                0
            }
        }
    }
    .into()
}
