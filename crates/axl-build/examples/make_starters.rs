//! Writes `assets/robots/*.axlbot` from the starter recipes and `missions/solutions/m12.axlbot`
//! from the m12 reference design: `cargo run -p axl-build --example make_starters`.
use axl_build::{io, starter};
use axl_parts::Catalog;

fn main() {
    let cat = Catalog::builtin();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("assets/robots");
    for (id, stem) in starter::STARTERS {
        match starter::build_starter(&cat, id) {
            Ok(a) => {
                let b = axl_build::Builder::with_assembly(cat.clone(), a.clone());
                for i in b.issues() {
                    println!("{id}: {:?} {} {:?}", i.level, i.message, i.parts);
                }
                io::save(&a, &dir.join(format!("{stem}.axlbot"))).expect("write");
                println!(
                    "{id}: {} parts, {} connections",
                    a.parts.len(),
                    b.connections().len()
                );
            }
            Err(e) => println!("{id}: FAILED {e}"),
        }
    }
    match starter::build_climber(&cat) {
        Ok(a) => {
            let b = axl_build::Builder::with_assembly(cat.clone(), a.clone());
            for i in b.issues() {
                println!("climber: {:?} {} {:?}", i.level, i.message, i.parts);
            }
            io::save(&a, &root.join("missions/solutions/m12.axlbot")).expect("write");
            println!("climber: {} parts", a.parts.len());
        }
        Err(e) => println!("climber: FAILED {e}"),
    }
}
