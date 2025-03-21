use heck::{ToShoutySnakeCase, ToUpperCamelCase};
use proc_macro2::{Literal, Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use syn::{Ident, LitInt, LitStr};

fn const_fluid_name_from_fluid_name(fluid: &str) -> String {
    fluid.to_shouty_snake_case()
}

fn property_group_name_from_derived_name(name: &str) -> String {
    format!("{}_fluid_properties", name).to_upper_camel_case()
}

struct PropertyVariantMapping {
    original_name: String,
    property_enum: String,
}

struct PropertyCollectionData {
    variant_mappings: Vec<PropertyVariantMapping>,
    fluid_names: Vec<String>,
}

impl PropertyCollectionData {
    pub fn add_fluid_name(&mut self, fluid_name: String) {
        self.fluid_names.push(fluid_name);
    }

    pub fn from_mappings(variant_mappings: Vec<PropertyVariantMapping>) -> Self {
        Self {
            variant_mappings,
            fluid_names: Vec::new(),
        }
    }

    pub fn derive_name(&self) -> String {
        format!("{}_like", self.fluid_names[0])
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct PropertyStruct {
    pub name: String,
    pub values: Vec<String>,
}

impl ToTokens for PropertyStruct {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let name = Ident::new(&self.name, Span::call_site());

        let variant_count = self.values.clone().len() as u16;
        let values_index = (0..self.values.clone().len() as u16).collect::<Vec<_>>();

        let ident_values = self
            .values
            .iter()
            .map(|value| {
                let value_str = if value.chars().all(|c| c.is_numeric()) {
                    format!("L{}", value)
                } else {
                    value.clone()
                };
                Ident::new(&value_str.to_upper_camel_case(), Span::call_site())
            });

        let values_2 = ident_values.clone();
        let values_3 = ident_values.clone();

        let from_values = self.values.iter().map(|value| {
            let value_str = if value.chars().all(|c| c.is_numeric()) {
                format!("L{}", value)
            } else {
                value.clone()
            };
            let ident = Ident::new(&value_str.to_upper_camel_case(), Span::call_site());
            quote! {
                #value => Self::#ident
            }
        });
        let to_values = self.values.iter().map(|value| {
            let value_str = if value.chars().all(|c| c.is_numeric()) {
                format!("L{}", value)
            } else {
                value.clone()
            };
            let ident = Ident::new(&value_str.to_upper_camel_case(), Span::call_site());
            quote! {
                Self::#ident => #value
            }
        });

        tokens.extend(quote! {
            #[derive(Clone, Copy, Debug, Eq, PartialEq)]
            pub enum #name {
                #(#ident_values),*
            }

            impl EnumVariants for #name {
                fn variant_count() -> u16 {
                    #variant_count
                }

                fn to_index(&self) -> u16 {
                    match self {
                        #(Self::#values_2 => #values_index),*
                    }
                }

                fn from_index(index: u16) -> Self {
                    match index {
                        #(#values_index => Self::#values_3,)*
                        _ => panic!("Invalid index: {}", index),
                    }
                }

                fn to_value(&self) -> &str {
                    match self {
                        #(#to_values),*
                    }
                }

                fn from_value(value: &str) -> Self {
                    match value {
                        #(#from_values),*,
                        _ => panic!("Invalid value: {:?}", value),
                    }
                }
            }
        });
    }
}

struct FluidPropertyStruct {
    data: PropertyCollectionData,
}

impl ToTokens for FluidPropertyStruct {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let struct_name = property_group_name_from_derived_name(&self.data.derive_name());
        let name = Ident::new(&struct_name, Span::call_site());

        let values = self.data.variant_mappings.iter().map(|entry| {
            let key = Ident::new_raw(&entry.original_name, Span::call_site());
            let value = Ident::new(&entry.property_enum, Span::call_site());

            quote! {
                #key: #value
            }
        });

        let fluid_names = &self.data.fluid_names;

        let field_names: Vec<_> = self
            .data
            .variant_mappings
            .iter()
            .rev()
            .map(|entry| Ident::new_raw(&entry.original_name, Span::call_site()))
            .collect();

        let field_types: Vec<_> = self
            .data
            .variant_mappings
            .iter()
            .rev()
            .map(|entry| Ident::new(&entry.property_enum, Span::call_site()))
            .collect();

        let to_props_values = self.data.variant_mappings.iter().map(|entry| {
            let key = &entry.original_name;
            let key2 = Ident::new_raw(&entry.original_name, Span::call_site());

            quote! {
                props.push((#key.to_string(), self.#key2.to_value().to_string()));
            }
        });

        let from_props_values = self.data.variant_mappings.iter().map(|entry| {
            let key = &entry.original_name;
            let key2 = Ident::new_raw(&entry.original_name, Span::call_site());
            let value = Ident::new(&entry.property_enum, Span::call_site());

            quote! {
                #key => fluid_props.#key2 = #value::from_value(&value)
            }
        });

        tokens.extend(quote! {
            #[derive(Clone, Copy, Debug, Eq, PartialEq)]
            pub struct #name {
                #(pub #values),*
            }

            impl FluidProperties for #name {
                #[allow(unused_assignments)]
                fn to_index(&self) -> u16 {
                    let mut index = 0;
                    let mut multiplier = 1;

                    #(
                        index += self.#field_names.to_index() * multiplier;
                        multiplier *= #field_types::variant_count();
                    )*

                    index
                }

                #[allow(unused_assignments)]
                fn from_index(mut index: u16) -> Self {
                    Self {
                        #(
                            #field_names: {
                                let value = index % #field_types::variant_count();
                                index /= #field_types::variant_count();
                                #field_types::from_index(value)
                            }
                        ),*
                    }
                }

                fn to_state_id(&self, fluid: &Fluid) -> u16 {
                    if ![#(#fluid_names),*].contains(&fluid.name) {
                        panic!("{} is not a valid fluid for {}", &fluid.name, #struct_name);
                    }

                    self.to_index() as u16
                }

                fn from_state_id(state_id: u16, fluid: &Fluid) -> Self {
                    if ![#(#fluid_names),*].contains(&fluid.name) {
                        panic!("{} is not a valid fluid for {}", &fluid.name, #struct_name);
                    }

                    Self::from_index(state_id)
                }

                fn default(fluid: &Fluid) -> Self {
                    if ![#(#fluid_names),*].contains(&fluid.name) {
                        panic!("{} is not a valid fluid for {}", &fluid.name, #struct_name);
                    }

                    Self::from_state_id(fluid.default_state_index as u16, fluid)
                }

                #[allow(clippy::vec_init_then_push)]
                fn to_props(&self) -> Vec<(String, String)> {
                    let mut props = vec![];

                    #(#to_props_values)*

                    props
                }

                fn from_props(props: Vec<(String, String)>, fluid: &Fluid) -> Self {
                    if ![#(#fluid_names),*].contains(&fluid.name) {
                        panic!("{} is not a valid fluid for {}", &fluid.name, #struct_name);
                    }

                    let mut fluid_props = Self::default(fluid);

                    for (key, value) in props {
                        match key.as_str() {
                            #(#from_props_values),*,
                            _ => panic!("Invalid key: {}", key),
                        }
                    }

                    fluid_props
                }
            }
        });
    }
}

#[derive(Deserialize, Clone)]
struct FluidState {
    height: f32,
    level: i32,
    is_empty: bool,
    blast_resistance: f32,
    block_state_id: u16,
    is_still: bool,
}

#[derive(Deserialize, Clone)]
struct Property {
    name: String,
    values: Vec<String>,
}

#[derive(Deserialize, Clone)]
struct Fluid {
    name: String,
    id: u8,
    properties: Vec<Property>,
    default_state_index: usize,
    states: Vec<FluidState>,
}

pub(crate) fn build() -> TokenStream {
    println!("cargo:rerun-if-changed=../assets/fluids.json");

    let fluids: Vec<Fluid> = serde_json::from_str(include_str!("../../assets/fluids.json"))
        .expect("Failed to parse fluids.json");

    let mut constants = TokenStream::new();
    let mut id_matches = Vec::new();
    let mut fluid_properties_from_state_and_name = TokenStream::new();
    let mut fluid_properties_from_props_and_name = TokenStream::new();

    // Used to create property `enum`s.
    let mut property_enums: HashMap<String, PropertyStruct> = HashMap::new();
    // Property implementation for a fluid.
    let mut fluid_properties: Vec<FluidPropertyStruct> = Vec::new();
    // Mapping of a collection of property names -> fluids that have these properties.
    let mut property_collection_map: HashMap<Vec<String>, PropertyCollectionData> = HashMap::new();
    // Validator that we have no `enum` collisions.
    let mut enum_to_values: HashMap<String, Vec<String>> = HashMap::new();

    for fluid in fluids.clone() {
        let id_name = LitStr::new(&fluid.name, proc_macro2::Span::call_site());
        let const_ident = format_ident!("{}", fluid.name.to_shouty_snake_case());

        let id_lit = LitInt::new(&fluid.id.to_string(), proc_macro2::Span::call_site());
        let mut properties = TokenStream::new();
        if fluid.properties.is_empty() {
            properties.extend(quote!(None));
        } else {
            let internal_properties = fluid.properties.iter().map(|property| {
                let key = LitStr::new(&property.name, proc_macro2::Span::call_site());
                let values = property
                    .values
                    .iter()
                    .map(|value| LitStr::new(value, proc_macro2::Span::call_site()));

                quote! {
                    (#key, &[
                        #(#values),*
                    ])
                }
            });
            properties.extend(quote! {
                Some(&[
                    #(#internal_properties),*
                ])
            });
        }

        let fluid_states = fluid.states.iter().map(|state| {
            let height = state.height;
            let level = state.level;
            let is_empty = state.is_empty;
            let blast_resistance = state.blast_resistance;
            let block_state_id = state.block_state_id;
            let is_still = state.is_still;
            quote! {
                FluidState {
                    height: #height,
                    level: #level,
                    is_empty: #is_empty,
                    blast_resistance: #blast_resistance,
                    block_state_id: #block_state_id,
                    is_still: #is_still,
                }
            }
        });
        let state_id = fluid.default_state_index as u8;

        id_matches.push(quote! {
            #id_name => Some(#id_lit),
        });

        constants.extend(quote! {
            pub const #const_ident: Fluid = Fluid {
                id: #id_lit,
                name: #id_name,
                properties: #properties,
                states: &[#(#fluid_states),*],
                default_state_index: #state_id
            };

        });

        let mut property_collection = HashSet::new();
        let mut property_mapping = Vec::new();
        for property in &fluid.properties {
            property_collection.insert(property.name.clone());

            // Get mapped property `enum` name
            let renamed_property = property.name.to_upper_camel_case();

            let expected_values = enum_to_values
                .entry(renamed_property.clone())
                .or_insert_with(|| property.values.clone());

            if expected_values != &property.values {
                panic!(
                    "Enum overlap for '{}' ({:?} vs {:?})",
                    property.name, &property.values, expected_values
                );
            };

            property_mapping.push(PropertyVariantMapping {
                original_name: property.name.clone(),
                property_enum: renamed_property.clone(),
            });

            // If this property doesn't have an `enum` yet, make one.
            let _ = property_enums
                .entry(renamed_property.clone())
                .or_insert_with(|| PropertyStruct {
                    name: renamed_property,
                    values: property.values.clone(),
                });
        }

        if !property_collection.is_empty() {
            let mut property_collection = Vec::from_iter(property_collection);
            property_collection.sort();
            property_collection_map
                .entry(property_collection)
                .or_insert_with(|| PropertyCollectionData::from_mappings(property_mapping))
                .add_fluid_name(fluid.name.clone());
        }
    }

    for property_group in property_collection_map.into_values() {
        for fluid_name in &property_group.fluid_names {
            let const_fluid_name = Ident::new(
                &const_fluid_name_from_fluid_name(fluid_name),
                Span::call_site(),
            );
            let property_name = Ident::new(
                &property_group_name_from_derived_name(&property_group.derive_name()),
                Span::call_site(),
            );

            fluid_properties_from_state_and_name.extend(quote! {
                #fluid_name => Some(Box::new(#property_name::from_state_id(state_id, &Fluid::#const_fluid_name))),
            });

            fluid_properties_from_props_and_name.extend(quote! {
                #fluid_name => Some(Box::new(#property_name::from_props(props, &Fluid::#const_fluid_name))),
            });
        }

        fluid_properties.push(FluidPropertyStruct {
            data: property_group,
        });
    }

    let fluid_props = fluid_properties.iter().map(|prop| prop.to_token_stream());
    let properties = property_enums.values().map(|prop| prop.to_token_stream());

    quote! {
        use crate::tag::{Tagable, RegistryKey};

        pub struct FluidState {
            pub height: f32,
            pub level: i32,
            pub is_empty: bool,
            pub blast_resistance: f32,
            pub block_state_id: u16,
            pub is_still: bool,
        }

        pub struct Fluid {
            pub id: u8,
            pub name: &'static str,
            pub properties: Option<&'static [(&'static str, &'static [&'static str])]>,
            pub states: &'static [FluidState],
            pub default_state_index: u8,
        }

        pub trait EnumVariants {
            fn variant_count() -> u16;
            fn to_index(&self) -> u16;
            fn from_index(index: u16) -> Self;
            fn to_value(&self) -> &str;
            fn from_value(value: &str) -> Self;
        }

        pub trait FluidProperties where Self: 'static {
            // Convert properties to an index (`0` to `N-1`).
            fn to_index(&self) -> u16;
            // Convert an index back to properties.
            fn from_index(index: u16) -> Self where Self: Sized;

            // Convert properties to a state id.
            fn to_state_id(&self, fluid: &Fluid) -> u16;
            // Convert a state id back to properties.
            fn from_state_id(state_id: u16, fluid: &Fluid) -> Self where Self: Sized;
            // Get the default properties.
            fn default(fluid: &Fluid) -> Self where Self: Sized;

            // Convert properties to a `Vec` of `(name, value)`
            fn to_props(&self) -> Vec<(String, String)>;

            // Convert properties to a fluid state, and add them onto the default state.
            fn from_props(props: Vec<(String, String)>, fluid: &Fluid) -> Self where Self: Sized;
        }

        impl Fluid {
            #constants

            pub fn ident_to_fluid_id(name: &str) -> Option<u8> {
                match name {
                    #(#id_matches)*
                    _ => None
                }
            }

            #[doc = r" Get the properties of the fluid."]
            pub fn properties(&self, state_id: u16) -> Option<Box<dyn FluidProperties>> {
                match self.name {
                    #fluid_properties_from_state_and_name
                    _ => None
                }
            }

            #[doc = r" Get the properties of the fluid."]
            pub fn from_properties(&self, props: Vec<(String, String)>) -> Option<Box<dyn FluidProperties>> {
                match self.name {
                    #fluid_properties_from_props_and_name
                    _ => None
                }
            }
        }

        impl Tagable for Fluid {
            #[inline]
            fn tag_key() -> RegistryKey {
                RegistryKey::Fluid
            }

            #[inline]
            fn registry_key(&self) -> &str {
                self.name
            }
        }

        #(#properties)*

        #(#fluid_props)*
    }
}
