//! This file includes all our resource IDs, and the code to load them. The
//! handy string_resources macro does the magic to create a StaticStringResource
//! for each of the resource IDs, and then we can use them in code.
//!
//! Example usage:
//! let world = r::IDS_WORLD.get();
//! println!("Hello: {}", world);

#![allow(dead_code)]
use win32resources::StaticStringResource;
macro_rules! string_resources {
    (
        $(
            $name:ident = $value:expr ;
        )*
    ) => {
        $(
            pub static $name: StaticStringResource = StaticStringResource::new($value, stringify!($name));
        )*
    }
}

include!("../../Generated Files/resource_ids.rs");
