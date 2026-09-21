use nemo_mcp::contract::{ApplicationRequest, ApplicationResponse};
fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments == ["--api-version", "2"] {
        print!(
            "{}",
            include_str!("../../engineering/application/native-transport-v2.schema.json")
        );
        return;
    }
    if !arguments.is_empty() {
        eprintln!("usage: nemo-mcp-schema [--api-version 2]");
        std::process::exit(2);
    }
    let schemas = serde_json::json!({
        "apiVersion": 1,
        "request": schemars::schema_for!(ApplicationRequest),
        "response": schemars::schema_for!(ApplicationResponse)
    });
    println!("{}", serde_json::to_string_pretty(&schemas).unwrap());
}
