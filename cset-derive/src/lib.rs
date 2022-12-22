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
        quote!(#ident: DraftField::Unchanged)
    });

    let changesetters = fields.iter().map(|field| {
        let UndoableStructField { id, ident, ty } = field;

        quote! {
            #id => {
                let new_value = change.take_old_value().downcast::<#ty>().unwrap();
                let old_value = ::std::mem::replace(&mut self.#ident, *new_value);

                changes.push(Change::new(
                    #id,
                    Box::new(old_value),
                ));
            }
        }
    });

    quote! {
        const _: () = {
            use ::cset::*;

            impl<'a> Trackable<'a> for #struct_ident {
                type Draft = #draft_ident<'a>;

                fn edit(&'a mut self) -> #draft_ident<'a> {
                    #draft_ident {
                        __backing: self,
                        #(#draft_fields,)*
                    }
                }

                fn apply_changeset(&mut self, changeset: ChangeSet) -> ChangeSet {
                    assert_eq!(changeset.target_type(), ::std::any::TypeId::of::<Self>());

                    let mut changes = Vec::new();

                    for change in changeset.take_changes() {
                        match change.field() {
                            #(#changesetters),*
                            _ => unreachable!("unknown field in change"),
                        }
                    }

                    ChangeSet::new(
                        ::std::any::TypeId::of::<Self>(),
                        changes,
                    )
                }
            }
        };

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

        quote! {
            pub fn #getter(&self) -> &#ty {
                match &self.#ident {
                    DraftField::Unchanged => &self.__backing.#ident,
                    DraftField::Changed(new_val) => &new_val,
                }
            }

            pub fn #setter(mut self, #ident: #ty) -> Self {
                self.#ident = DraftField::Changed(#ident);
                self
            }

            pub fn #dirty_checker(&self) -> bool {
                matches!(&self.#ident, DraftField::Changed(_))
            }

            pub fn #resetter(&mut self) -> Option<#ty> {
                match ::std::mem::replace(&mut self.#ident, DraftField::Unchanged) {
                    DraftField::Changed(new_value) => {
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
            if let DraftField::Changed(#ident) = self.#ident {
                let old_value = ::std::mem::replace(&mut self.__backing.#ident, #ident);

                changes.push(Change::new(
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

        const _: () = {
            use ::cset::*;

            impl<'a> #draft_ident<'a> {
                #(#draft_field_fns)*

                pub fn is_dirty(&self) -> bool {
                    #(#draft_change_checkers)||*
                }

                pub fn reset(&mut self) {
                    #(#draft_resetters;)*
                }
            }

            impl<'a> Draft<'a> for #draft_ident<'a> {
                fn commit(mut self) -> ChangeSet {
                    let mut changes: Vec<Change> = Vec::new();

                    #(#draft_field_commit)*

                    ChangeSet::new(
                        ::std::any::TypeId::of::<#struct_ident>(),
                        changes
                    )
                }
            }
        };
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
