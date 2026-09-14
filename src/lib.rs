pub mod brandi;
pub mod database;
// `demand` was added (as an uncommitted, never-wired-in module) in the same
// commit that deleted the rest of this crate, so it never got hooked into
// `gates::gate_5_demand_evidence` or the `evolve propose` CLI flow before
// being lost. It compiles and its own unit tests pass standalone; wiring it
// into the gate pipeline would mean inventing that integration from
// scratch, which is out of scope for a restoration — left for follow-up.
pub mod demand;
pub mod discovery;
pub mod frontier;
pub mod gates;
pub mod goblin;
pub mod inference;
pub mod output;
pub mod padagonia;
pub mod project_reader;
pub mod scoring;
pub mod search_generator;
pub mod trajectory;
pub mod transformation;
pub mod types;
