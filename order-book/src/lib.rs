pub mod defs {
    pub mod items {
        include!(concat!(env!("OUT_DIR"), "/order.rs"));
    }
}

pub mod book;
pub mod configuration;
