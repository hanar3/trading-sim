pub mod configuration;
pub mod defs {
    pub mod items {
        include!(concat!(env!("OUT_DIR"), "/order.rs"));
    }
}
