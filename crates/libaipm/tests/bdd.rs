use cucumber::World;

#[derive(Debug, Default, World)]
pub struct AipmWorld;

fn main() {
    let features_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/features");
    futures::executor::block_on(AipmWorld::run(features_dir));
}
