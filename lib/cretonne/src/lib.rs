//! Cretonne code generation library.

#![deny(missing_docs)]

pub use context::Context;
pub use legalizer::legalize_function;
pub use verifier::verify_function;
pub use write::write_function;

/// Version number of the cretonne crate.
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[macro_use]
pub mod dbg;
#[macro_use]
pub mod entity_ref;

pub mod binemit;
pub mod bitset;
pub mod dominator_tree;
pub mod entity_list;
pub mod entity_map;
pub mod flowgraph;
pub mod frontend;
pub mod ir;
pub mod isa;
pub mod loop_analysis;
pub mod regalloc;
pub mod result;
pub mod settings;
pub mod sparse_map;
pub mod ssa;
pub mod verifier;

mod abi;
mod constant_hash;
mod context;
mod iterators;
mod legalizer;
mod licm;
mod packed_option;
mod partition_slice;
mod predicates;
mod ref_slice;
mod simple_gvn;
mod topo_order;
mod write;
