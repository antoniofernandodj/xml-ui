//! Proc-macro backing `xml_ui::component`.
//!
//! `#[component(path = "...", name = "...")]` on a struct reads the XML template
//! at compile time, extracts its `<script>` block (real Rust), and generates the
//! `impl Component`:
//!   - every `fn` in the script becomes an inherent method and an action of the
//!     same name (matched against `onClick`/`onChange`);
//!   - a method taking an extra argument receives the input value (`&str`);
//!   - every named struct field is synced into the engine context (via
//!     `to_string()`) on `init` and after each action, so `{field}` bindings
//!     update.

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, punctuated::Punctuated, Expr, ExprLit, Fields, FnArg, ImplItem, ItemImpl,
    ItemStruct, Lit, MetaNameValue, Token,
};

/// Splits `<script>...</script>` out of XML, returning the script body if found.
/// Mirrors `xml_ui::strip_script` (the macro can't depend on the engine crate).
fn extract_script(xml: &str) -> Option<String> {
    let lower = xml.to_ascii_lowercase();
    let open = lower.find("<script")?;
    let gt = lower[open..].find('>')? + open + 1;
    let close = lower[gt..].find("</script>")? + gt;
    Some(xml[gt..close].to_string())
}

#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr with Punctuated::<MetaNameValue, Token![,]>::parse_terminated);
    let item_struct = parse_macro_input!(item as ItemStruct);
    let ident = item_struct.ident.clone();

    // --- Read `path` and optional `name` from the attribute -----------------
    let mut path: Option<String> = None;
    let mut name: Option<String> = None;
    for nv in &args {
        let lit = match &nv.value {
            Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) => s.value(),
            _ => return err(nv.value.span(), "expected a string literal"),
        };
        if nv.path.is_ident("path") {
            path = Some(lit);
        } else if nv.path.is_ident("name") {
            name = Some(lit);
        } else {
            return err(nv.path.span(), "unknown key; expected `path` or `name`");
        }
    }
    let Some(path) = path else {
        return err(ident.span(), "#[component] requires `path = \"...\"`");
    };
    let name = name.unwrap_or_else(|| ident.to_string());

    // --- Read the template file at compile time -----------------------------
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let full_path = std::path::Path::new(&manifest).join(&path);
    let xml = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(e) => {
            return err(
                ident.span(),
                &format!("failed to read template '{}': {}", full_path.display(), e),
            )
        }
    };

    // --- Parse the `<script>` methods ---------------------------------------
    let script = extract_script(&xml).unwrap_or_default();
    let wrapped = format!("impl {ident} {{ {script} }}");
    let methods_impl: ItemImpl = match syn::parse_str(&wrapped) {
        Ok(i) => i,
        Err(e) => return err(ident.span(), &format!("failed to parse <script>: {e}")),
    };

    // Each method becomes a match arm keyed by its name. A method with an extra
    // (non-receiver) parameter is treated as an input handler and gets the value.
    let arms = methods_impl.items.iter().filter_map(|it| {
        let ImplItem::Fn(f) = it else { return None };
        let m = &f.sig.ident;
        let key = m.to_string();
        let takes_value = f.sig.inputs.iter().any(|a| matches!(a, FnArg::Typed(_)));
        Some(if takes_value {
            quote! { #key => self.#m(value.unwrap_or_default()), }
        } else {
            quote! { #key => { self.#m(); } }
        })
    });

    // --- Sync named struct fields into the context --------------------------
    let field_idents: Vec<_> = match &item_struct.fields {
        Fields::Named(named) => named.named.iter().filter_map(|f| f.ident.clone()).collect(),
        _ => Vec::new(),
    };
    let sync = quote! {
        #( ctx.set(stringify!(#field_idents), self.#field_idents.to_string()); )*
    };

    let expanded = quote! {
        #item_struct
        #methods_impl

        impl ::xml_ui::Component for #ident {
            fn name(&self) -> &str { #name }

            fn template(&self) -> ::xml_ui::Template {
                ::xml_ui::Template::File(#path.into())
            }

            fn init(&mut self, ctx: &mut ::xml_ui::Context) {
                #sync
            }

            fn update(
                &mut self,
                action: &str,
                value: ::core::option::Option<&str>,
                ctx: &mut ::xml_ui::Context,
            ) {
                let _ = value;
                match action {
                    #( #arms )*
                    _ => {}
                }
                #sync
            }
        }
    };
    expanded.into()
}

/// Emits a `compile_error!` at the given span.
fn err(span: proc_macro2::Span, msg: &str) -> TokenStream {
    syn::Error::new(span, msg).to_compile_error().into()
}

// Bring `.span()` into scope for the helpers above.
use syn::spanned::Spanned;
