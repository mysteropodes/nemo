use nemo_mcp::contract::{ApplicationRequest, ApplicationResponse};
fn main() {
    let schemas = serde_json::json!({
        "apiVersion": 1,
        "request": schemars::schema_for!(ApplicationRequest),
        "response": schemars::schema_for!(ApplicationResponse)
    });
    println!("{}", serde_json::to_string_pretty(&schemas).unwrap());
}
