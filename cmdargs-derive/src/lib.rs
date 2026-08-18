//! Derive macro `#[derive(CmdArgs)]` for L_builtin subcommands.
//!
//! Each field of the annotated struct maps to a command-line piece:
//!
//! - `#[opt('c')]`      value option `-c VALUE` (`-c` takes an argument).
//! - `#[flag('c')]`     boolean flag `-c` (no argument).
//! - `#[positional]`    required positional argument.
//! - `#[optional]`      optional positional argument (`Option<T>`).
//! - `#[rest]`          variadic positional handed back as the raw remaining
//!                      `*mut WORD_LIST` (no `FromCpnt` conversion). Used by
//!                      dispatch subcommands that forward the leftover words to
//!                      a child handler with the C ABI `(*mut WORD_LIST)` shape,
//!                      and by any subcommand that needs to iterate the
//!                      remaining words directly.
//! - `#[flatten]`       embed another `CmdArgs` struct (its options/positionals
//!                      are merged into the parent).
//! - `#[parse(expr)]`   custom converter `Fn(Cpnt) -> Result<T, E>` (E: Display),
//!                      overrides the default `FromCpnt` conversion.
//!
//! The generated `parse(list)` drives bash's `internal_getopt` exactly once over
//! the `*mut WORD_LIST`, dispatches value options into their fields via
//! `FromCpnt` (zero-copy for `*const c_char` / `Cpnt`), then binds the remaining
//! words into the positional fields.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Expr, Fields, LitChar, LitStr};

#[proc_macro_derive(
    CmdArgs,
    attributes(opt, flag, positional, optional, rest, parse, flatten)
)]
pub fn derive_cmd_args(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("CmdArgs only supports structs with named fields"),
        },
        _ => panic!("CmdArgs only supports structs"),
    };

    #[allow(dead_code)]
    enum FieldKind {
        Opt(char),
        Flag(char),
        Positional,
        Optional(Option<Expr>),
        Rest,
        Flatten,
    }

    struct FieldInfo {
        ident: syn::Ident,
        ty: syn::Type,
        kind: FieldKind,
        parser: Option<Expr>,
    }

    let mut infos = Vec::new();

    for field in fields {
        let ident = field.ident.clone().unwrap();
        let ty = field.ty.clone();

        let mut kind = FieldKind::Positional;
        let mut parser = None;

        for attr in &field.attrs {
            if attr.path().is_ident("flatten") {
                kind = FieldKind::Flatten;
            } else if attr.path().is_ident("rest") {
                kind = FieldKind::Rest;
            } else if attr.path().is_ident("opt") {
                let ch = attr.parse_args::<LitChar>().unwrap().value();
                kind = FieldKind::Opt(ch);
            } else if attr.path().is_ident("flag") {
                let ch = attr.parse_args::<LitChar>().unwrap().value();
                kind = FieldKind::Flag(ch);
            } else if attr.path().is_ident("optional") {
                let mut def_expr = None;
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("default") {
                        def_expr = Some(meta.value()?.parse::<Expr>()?);
                    }
                    Ok(())
                });
                kind = FieldKind::Optional(def_expr);
            } else if attr.path().is_ident("parse") {
                parser = Some(attr.parse_args::<Expr>().unwrap());
            }
        }

        infos.push(FieldInfo { ident, ty, kind, parser });
    }

    let mut optstring_pieces: Vec<LitStr> = Vec::new();
    let mut has_rest = false;
    let mut default_inits = Vec::new();
    let mut apply_opt_own = Vec::new();
    let mut apply_opt_flatten = Vec::new();
    let mut fill_stmts = Vec::new();
    let mut rest_ptr_stmts = Vec::new();
    let mut flatten_inherit_macro_calls: Vec<proc_macro2::TokenStream> = Vec::new();

    for info in &infos {
        let ident = &info.ident;
        let ty = &info.ty;

        let conv = |cpnt: proc_macro2::TokenStream| match &info.parser {
            Some(p) => quote!(parse_with(#cpnt, #p)),
            None => quote!(FromCpnt::from_cpnt(#cpnt)),
        };
        let opt_conv = conv(quote!(Cpnt::new(p)));
        let pos_conv = conv(quote!(cptr));

        let err_arm = quote! {
            Err(e) => {
                let msg = e.to_string();
                crate::l_builtin_usage_error!(msg.as_bytes());
                return ::core::result::Result::Err(EX_USAGE);
            }
        };

        match &info.kind {
            FieldKind::Opt(ch) => {
                let ch_lit = *ch;
                optstring_pieces.push(LitStr::new(&format!("{ch}:").to_string(), Span::call_site()));
                let assign = quote! {
                    self.#ident = ::core::option::Option::Some(match #opt_conv {
                        ::core::result::Result::Ok(v) => v,
                        #err_arm
                    });
                };
                apply_opt_own.push(quote! {
                    if c == (#ch_lit as c_int) {
                        #assign
                    }
                });
                default_inits.push(quote!(#ident: ::core::option::Option::None));
            }
            FieldKind::Flag(ch) => {
                let ch_lit = *ch;
                optstring_pieces.push(LitStr::new(&format!("{ch}").to_string(), Span::call_site()));
                apply_opt_own.push(quote! {
                    if c == (#ch_lit as c_int) {
                        self.#ident = true;
                    }
                });
                default_inits.push(quote!(#ident: false));
            }
            FieldKind::Positional => {
                fill_stmts.push(quote! {
                    {
                        let cptr = match iter.next() {
                            ::core::option::Option::Some(x) => x,
                            ::core::option::Option::None => {
                                crate::l_builtin_usage_error!(
                                    concat!("missing required argument: ", stringify!(#ident)).as_bytes()
                                );
                                return ::core::result::Result::Err(EX_USAGE);
                            }
                        };
                        self.#ident = match #pos_conv {
                            ::core::result::Result::Ok(v) => v,
                            #err_arm
                        };
                    }
                });
                default_inits.push(quote!(#ident: ::core::default::Default::default()));
            }
            FieldKind::Optional(def) => {
                match def {
                    Some(d) => {
                        fill_stmts.push(quote! {
                            if let ::core::option::Option::Some(cptr) = iter.next() {
                                self.#ident = match #pos_conv {
                                    ::core::result::Result::Ok(v) => v,
                                    #err_arm
                                };
                            } else {
                                self.#ident = #d;
                            }
                        });
                        default_inits.push(quote!(#ident: #d));
                    }
                    None => {
                        fill_stmts.push(quote! {
                            if let ::core::option::Option::Some(cptr) = iter.next() {
                                self.#ident = ::core::option::Option::Some(match #pos_conv {
                                    ::core::result::Result::Ok(v) => v,
                                    #err_arm
                                });
                            }
                        });
                        default_inits.push(quote!(#ident: ::core::option::Option::None));
                    }
                }
            }
            FieldKind::Rest => {
                has_rest = true;
                // Captured after every other positional so the view spans the
                // words remaining once positionals are consumed. The lifetime is
                // erased to 'static: cmdargs is only ever driven by bash for the
                // duration of a single builtin invocation, and the WORD_LIST is
                // owned by bash for at least that long.
                rest_ptr_stmts.push(quote! {
                    self.#ident = unsafe {
                        ::core::mem::transmute::<WordListIterCpnt<'_>, WordListIterCpnt<'static>>(
                            WordListView::from_raw(iter.as_ptr()).iter(),
                        )
                    };
                });
                default_inits.push(quote! {
                    #ident: unsafe {
                        ::core::mem::transmute::<WordListIterCpnt<'_>, WordListIterCpnt<'static>>(
                            WordListView::from_raw(::std::ptr::null_mut()).iter(),
                        )
                    }
                });
            }
            FieldKind::Flatten => {
                let child_ident = match ty {
                    syn::Type::Path(p) => p.path.segments.last().unwrap().ident.clone(),
                    _ => panic!("flatten field type must be a path to a CmdArgs struct"),
                };
                let child_mac =
                    syn::Ident::new(&format!("__cmdargs_inherit_{child_ident}"), Span::call_site());
                flatten_inherit_macro_calls.push(quote!(#child_mac!()));
                apply_opt_flatten.push(quote! {
                    CmdArgs::apply_opt(&mut self.#ident, c, p)?;
                });
                fill_stmts.push(quote! {
                    CmdArgs::fill_positionals(&mut self.#ident, iter)?;
                });
                default_inits.push(quote!(
                    #ident: <#ty as CmdArgs>::new_default()
                ));
            }
        }
    }

    // Each CmdArgs struct exposes its own + all flattened descendants' option
    // characters as a `macro_rules!` that expands to a literal. This lets a
    // `#[flatten]` parent build its full optstring at COMPILE TIME via
    // `concat!` (concat! only accepts literals, never const `&str` values), so
    // `parse` never allocates a `String` to merge option characters.
    let mut concat_args: Vec<proc_macro2::TokenStream> = Vec::new();
    for p in &optstring_pieces {
        concat_args.push(quote!(#p));
    }
    for cm in &flatten_inherit_macro_calls {
        concat_args.push(quote!(#cm));
    }

    let inherit_macro_name =
        syn::Ident::new(&format!("__cmdargs_inherit_{struct_name}"), Span::call_site());
    let inherit_macro = if concat_args.is_empty() {
        quote! {
            #[macro_export]
            macro_rules! #inherit_macro_name { () => { "" }; }
        }
    } else {
        quote! {
            #[macro_export]
            macro_rules! #inherit_macro_name {
                () => { concat!( #(#concat_args),* ) };
            }
        }
    };

    let expanded = quote! {
        use crate::cmdargs::*;

        #inherit_macro

        impl #struct_name {
            #[inline]
            pub unsafe fn parse(
                list: *mut crate::cmdargs::WORD_LIST,
            ) -> ::core::result::Result<Self, crate::cmdargs::c_int> {
                <Self as crate::cmdargs::CmdArgs>::parse(list)
            }
        }

        impl CmdArgs for #struct_name {
            const OPTSTRING: &'static str = concat!(#inherit_macro_name!(), "h\0");
            const HAS_REST: bool = #has_rest;

            fn new_default() -> Self {
                Self {
                    #(#default_inits),*
                }
            }

            unsafe fn apply_opt(
                &mut self,
                c: c_int,
                p: *mut c_char,
            ) -> ::core::result::Result<(), c_int> {
                #(#apply_opt_own)*
                #(#apply_opt_flatten)*
                ::core::result::Result::Ok(())
            }

            unsafe fn fill_positionals(
                &mut self,
                iter: &mut WordListIterCpnt,
            ) -> ::core::result::Result<(), c_int> {
                #(#fill_stmts)*
                #(#rest_ptr_stmts)*
                ::core::result::Result::Ok(())
            }
        }
    };

    TokenStream::from(expanded)
}
