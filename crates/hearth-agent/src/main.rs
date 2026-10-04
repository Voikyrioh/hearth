//! Agent Hearth : service installé sur le serveur piloté.

fn main() {
    println!(
        "hearth-agent {} (api v{})",
        env!("CARGO_PKG_VERSION"),
        hearth_proto::version::API_VERSION
    );
}
