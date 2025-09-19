use std::fs;

use rs_project_startup::api::http::server::base_openapi_doc;

fn main() {
    let doc = base_openapi_doc();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    fs::write("openapi.json", json).unwrap();
}
