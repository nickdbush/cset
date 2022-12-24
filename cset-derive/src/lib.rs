use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Attribute, DataStruct, DeriveInput, Error, Meta, NestedMeta, Type};

#[proc_macro_derive(Track, attributes(track))]
pub fn macro_entry(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let expanded = match &input.data {
        syn::Data::Struct(data) => derive_tracked_struct(&input, data),
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

struct TrackedField {
    index: usize,
    ident: Ident,
    ty: Type,
    flattened_ident: Option<Ident>,
}

fn derive_tracked_struct(input: &DeriveInput, data: &DataStruct) -> TokenStream {
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
        .enumerate()
        .map(|(index, field)| {
            let ident = field.ident.clone().unwrap();
            let is_flattened = field.attrs.iter().any(|attr| {
                get_meta_items(attr).unwrap().iter().any(|meta| match meta {
                    NestedMeta::Meta(Meta::Path(path)) => path.is_ident("flatten"),
                    _ => false,
                })
            });
            
            let ty = field.ty.clone();
            let flattened_ident = if is_flattened {
                Some(flattened_struct_ident(&ty))
            } else {
                None
            };

            TrackedField {
                index,
                ident,
                ty: field.ty.clone(),
                flattened_ident,
            }
        })
        .collect::<Vec<_>>();

    let draft_struct = derive_draft_struct(struct_ident, &fields[..]);
    let draft_ident = create_draft_ident(struct_ident);
    let draft_setters = fields.iter().map(|field| {
        let TrackedField {
            ident,
            flattened_ident,
            ..
        } = field;

        if flattened_ident.is_some() {
            quote!(#ident: self.#ident.edit())
        } else {
            quote!(#ident: ::cset::DraftField::new(&mut self.#ident))
        }
    });

    let apply_value_fields = fields
        .iter()
        .filter(|field| field.flattened_ident.is_none())
        .map(|field| {
            let TrackedField {
                index, ident, ty, ..
            } = field;

            quote! {
                #index => {
                    let new_value = *value.downcast::<#ty>().unwrap();
                    let old_value = ::std::mem::replace(&mut self.#ident, new_value);
                    reverse_changes.push(::cset::Change {
                        field_id: change.field_id,
                        value: ::cset::ChangeValue::Value(::std::boxed::Box::new(old_value)),
                    });
                }
            }
        });

    let apply_changeset_fields = fields
        .iter()
        .filter(|field| field.flattened_ident.is_some())
        .map(|field| {
            let TrackedField {
                index, ident, ..
            } = field;

            quote! {
                #index => {
                    let reverse_change = self.#ident.apply_impl(field_changes, depth + 1);
                    reverse_changes.push(::cset::Change {
                        field_id: change.field_id,
                        value: ::cset::ChangeValue::ChangeSet(reverse_change),
                    });
                }
            }
        });

    quote! {
        impl #struct_ident {
            pub fn edit(&mut self) -> #draft_ident {
                #draft_ident {
                    #(#draft_setters,)*
                }
            }

            pub fn apply(&mut self, changeset: ::cset::ChangeSet) -> ::cset::ChangeSet {
                self.apply_impl(changeset, 0)
            }

            fn apply_impl(&mut self, changeset: ::cset::ChangeSet, depth: usize) -> ::cset::ChangeSet {
                assert!(changeset.for_type::<#struct_ident>());
                let mut reverse_changes = Vec::new();

                for change in changeset.changes {
                    let field_index = change.field_id.field_index(depth);

                    match change.value {
                        ::cset::ChangeValue::Value(value) => match field_index {
                            #(#apply_value_fields,)*
                            _ => unreachable!(),
                        },
                        ::cset::ChangeValue::ChangeSet(field_changes) => match field_index {
                            #(#apply_changeset_fields,)*
                            _ => unreachable!(),
                        },
                    };
                }

                ::cset::ChangeSet::new::<#struct_ident>(reverse_changes)
            }
        }

        #draft_struct
    }
}

fn derive_draft_struct(struct_ident: &Ident, fields: &[TrackedField]) -> TokenStream {
    let draft_ident = create_draft_ident(struct_ident);

    let draft_fields = fields.iter().map(|field| {
        let TrackedField { ident, ty, flattened_ident, .. } = field;

        if let Some(flattened_ident) = flattened_ident {
            let draft_ident = create_draft_ident(flattened_ident);
            quote!(#ident: #draft_ident<'b>)
        } else {
            quote!(#ident: ::cset::DraftField::<'b, #ty>)
        }
    });

    let field_api_fns = fields.iter().map(|field| {
        let TrackedField { ident, ty, flattened_ident, .. } = field;
        let dirty_checker = create_dirty_check_ident(ident);
        let resetter = create_resetter_ident(ident);
                
        if let Some(flattened_ident) = flattened_ident {
            let editor = format_ident!("edit_{ident}");
            let flattened_draft_ident = create_draft_ident(flattened_ident); 
            quote! {
                pub fn #editor(&mut self) -> &mut #flattened_draft_ident<'b> {
                    &mut self.#ident
                }

                pub fn #dirty_checker(&self) -> bool {
                    self.#ident.is_dirty()
                }

                pub fn #resetter(&mut self) {
                    self.#ident.reset();
                }
            }
        } else {
            let getter = format_ident!("get_{ident}");
            let setter = format_ident!("set_{ident}");
            quote! {
                pub fn #getter(&self) -> &#ty {
                    if let Some(#ident) = &self.#ident.draft {
                        #ident
                    } else {
                        &self.#ident.original
                    }
                }

                pub fn #setter(&mut self, #ident: #ty) {
                    self.#ident.draft = Some(#ident);
                }

                pub fn #dirty_checker(&self) -> bool {
                    self.#ident.draft.is_some()
                }

                pub fn #resetter(&mut self) -> Option<#ty> {
                    self.#ident.draft.take()
                }
            }
        }
    });

    let draft_change_checkers = fields.iter().map(|field| {
        let TrackedField { ident, .. } = field;
        let dirty_checker = create_dirty_check_ident(ident);
        quote!(self.#dirty_checker())
    });

    let draft_resetters = fields.iter().map(|field| {
        let TrackedField { ident, .. } = field;
        let resetter = create_resetter_ident(ident);
        quote!(self.#resetter())
    });

    let field_commits = fields.iter().map(|field| {
        let TrackedField { index, ident, flattened_ident, .. } = field;

        if flattened_ident.is_some() {
            quote! {
                {
                    let new_field_idx = field_idx.push_field(#index);
                    changes.push(::cset::Change {
                        field_id: new_field_idx.clone(),
                        value: ::cset::ChangeValue::ChangeSet(self.#ident.apply_impl(new_field_idx)),
                    });
                }
            }
        } else {   
            quote! {
                if let Some(change) = self.#ident.apply(field_idx.push_field(#index)) {
                    changes.push(change);
                }
            }
        }
    });

    quote! {
        pub struct #draft_ident<'b> {
            #(#draft_fields,)*
        }

        impl<'b> #draft_ident<'b> {
            #(#field_api_fns)*

            /// Returns true if the draft will modify the underlying struct if
            /// committed.
            pub fn is_dirty(&self) -> bool {
                #(#draft_change_checkers)||*
            }

            /// Clear all updates to changed fields.
            pub fn reset(&mut self) {
                #(#draft_resetters;)*
            }

            pub fn apply(self) -> ::cset::ChangeSet {
                self.apply_impl(::cset::FieldId::default())
            }
    
            fn apply_impl(self, field_idx: ::cset::FieldId) -> ::cset::ChangeSet {
                let mut changes = Vec::new();
    
                #(#field_commits)*
    
                ::cset::ChangeSet::new::<#struct_ident>(changes)
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

fn get_meta_items(attr: &Attribute) -> syn::Result<Vec<NestedMeta>> {
    if attr.path.is_ident("track") {
        match attr.parse_meta()? {
            Meta::List(meta) => Ok(Vec::from_iter(meta.nested)),
            bad => Err(Error::new_spanned(bad, "unrecognized attribute")),
        }
    } else {
        Ok(Vec::new())
    }
}

fn flattened_struct_ident(ty: &Type) -> Ident {
    match ty {
        Type::Path(path) => {
            path.path.get_ident().unwrap().clone()
        },
        _ => todo!(),
    }
}