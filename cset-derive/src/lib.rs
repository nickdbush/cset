use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, DataStruct, DeriveInput, Type};

#[proc_macro_derive(Track)]
pub fn macro_entry(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let expanded = match &input.data {
        syn::Data::Struct(data) => derive_struct_undo(&input, data),
        syn::Data::Enum(data) => {
            syn::Error::new_spanned(data.enum_token, "Cannot derive Undo for enums")
                .into_compile_error()
        }
        syn::Data::Union(data) => {
            syn::Error::new_spanned(data.union_token, "Cannot derive Undo for unions")
                .into_compile_error()
        }
    };

    expanded.into()
}

struct UndoableStructField {
    id: String,
    ident: Ident,
    ty: Type,
}

fn derive_struct_undo(input: &DeriveInput, data: &DataStruct) -> TokenStream {
    let struct_ident = &input.ident;

    for field in &data.fields {
        if field.ident.is_none() {
            return syn::Error::new_spanned(&data.fields, "Cannot derive Undo for tuple structs")
                .to_compile_error();
        }
    }

    let fields = data
        .fields
        .iter()
        .map(|field| {
            let ident = field.ident.clone().unwrap();
            UndoableStructField {
                id: ident.to_string(),
                ident,
                ty: field.ty.clone(),
            }
        })
        .collect::<Vec<_>>();

    let draft_struct = create_draft_struct(struct_ident, &fields[..]);
    let draft_ident = create_draft_ident(struct_ident);
    let draft_fields = fields.iter().map(|field| {
        let UndoableStructField { ident, .. } = field;
        quote!(#ident: ::cset::DraftField::Unchanged)
    });

    let changesetters = fields.iter().map(|field| {
        let UndoableStructField { id, ident, ty } = field;

        quote! {
            #id => {
                let new_value = change.take_old_value().downcast::<#ty>().unwrap();
                let old_value = ::std::mem::replace(&mut self.#ident, *new_value);

                changes.push(::cset::Change::new(
                    #id,
                    Box::new(old_value),
                ));
            }
        }
    });

    quote! {
        impl<'a> Trackable<'a> for #struct_ident {
            type Draft = #draft_ident<'a>;

            fn edit(&'a mut self) -> #draft_ident<'a> {
                #draft_ident {
                    __backing: self,
                    #(#draft_fields,)*
                }
            }

            fn apply_changeset(&mut self, changeset: ::cset::ChangeSet) -> ::cset::ChangeSet {
                assert_eq!(changeset.target_type(), ::std::any::TypeId::of::<Self>());

                let mut changes = Vec::new();

                for change in changeset.take_changes() {
                    match change.field() {
                        #(#changesetters),*
                        _ => unreachable!("unknown field in change"),
                    }
                }

                ::cset::ChangeSet::new(
                    ::std::any::TypeId::of::<Self>(),
                    changes,
                )
            }
        }

        #draft_struct
    }
}

fn create_draft_struct(struct_ident: &Ident, fields: &[UndoableStructField]) -> TokenStream {
    let draft_ident = create_draft_ident(struct_ident);

    let draft_fields = fields.iter().map(|field| {
        let UndoableStructField { ident, ty, .. } = field;

        quote! {
            #ident: ::cset::DraftField::<#ty>
        }
    });

    let draft_field_fns = fields.iter().map(|field| {
        let UndoableStructField { ident, ty, .. } = field;
        let getter = format_ident!("get_{ident}");
        let setter = format_ident!("set_{ident}");
        let dirty_checker = create_dirty_check_ident(ident);
        let resetter = create_resetter_ident(ident);

        let struct_field = format!("[`{struct_ident}::{ident}`]");
        let commit_method = "[`Self::commit()`]".to_string();
        let getter_doc = format!("Gets the value for {struct_field}.\n\nReturns a reference to the draft value if set, or falls through to the underlying struct's value.");
        let setter_doc = format!("Set a draft value for {struct_field}.\n\nThis method does not overwrite the existing value in the underlying struct. To apply the change to the underlying struct, call {commit_method}.");
        let dirty_doc = format!("Returns whether {struct_field} would be changed by this draft if {commit_method} was called.");
        let reset_doc = format!("Clear any draft changes made to {struct_field}.");

        quote! {
            #[doc = #getter_doc]
            pub fn #getter(&self) -> &#ty {
                match &self.#ident {
                    ::cset::DraftField::Unchanged => &self.__backing.#ident,
                    ::cset::DraftField::Changed(new_val) => &new_val,
                }
            }

            #[doc = #setter_doc]
            pub fn #setter(mut self, #ident: #ty) -> Self {
                self.#ident = ::cset::DraftField::Changed(#ident);
                self
            }

            #[doc = #dirty_doc]
            pub fn #dirty_checker(&self) -> bool {
                matches!(&self.#ident, ::cset::DraftField::Changed(_))
            }

            #[doc = #reset_doc]
            pub fn #resetter(&mut self) -> Option<#ty> {
                match ::std::mem::replace(&mut self.#ident, ::cset::DraftField::Unchanged) {
                    ::cset::DraftField::Changed(new_value) => {
                        Some(new_value)
                    }
                    _ => None,
                }
            }
        }
    });

    let draft_change_checkers = fields.iter().map(|field| {
        let UndoableStructField { ident, .. } = field;
        let dirty_checker = create_dirty_check_ident(ident);
        quote!(self.#dirty_checker())
    });

    let draft_resetters = fields.iter().map(|field| {
        let UndoableStructField { ident, .. } = field;
        let resetter = create_resetter_ident(ident);
        quote!(self.#resetter())
    });

    let draft_field_commit = fields.iter().map(|field| {
        let UndoableStructField { id, ident, .. } = field;

        quote! {
            if let ::cset::DraftField::Changed(#ident) = self.#ident {
                let old_value = ::std::mem::replace(&mut self.__backing.#ident, #ident);

                changes.push(::cset::Change::new(
                    #id,
                    Box::new(old_value),
                ))
            }
        }
    });

    quote! {
        pub struct #draft_ident<'a> {
            __backing: &'a mut #struct_ident,
            #(#draft_fields,)*
        }

        impl<'a> #draft_ident<'a> {
            #(#draft_field_fns)*

            /// Returns true if the draft will modify the underlying struct if
            /// committed.
            pub fn is_dirty(&self) -> bool {
                #(#draft_change_checkers)||*
            }

            /// Clear all updates to changed fields.
            pub fn reset(&mut self) {
                #(#draft_resetters;)*
            }
        }

        impl<'a> Draft<'a> for #draft_ident<'a> {
            fn commit(mut self) -> ChangeSet {
                let mut changes = Vec::new();

                #(#draft_field_commit)*

                ChangeSet::new(
                    ::std::any::TypeId::of::<#struct_ident>(),
                    changes
                )
            }
        }
    }
}

fn create_draft_ident(ident: &Ident) -> Ident {
    format_ident!("{ident}Draft")
}

fn create_dirty_check_ident(ident: &Ident) -> Ident {
    format_ident!("is_{ident}_dirty")
}

fn create_resetter_ident(ident: &Ident) -> Ident {
    format_ident!("reset_{ident}")
}
