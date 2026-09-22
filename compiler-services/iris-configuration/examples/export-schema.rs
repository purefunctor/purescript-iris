use std::error::Error;
use std::path::Path;
use std::{env, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let schema = serde_json::to_string_pretty(&iris_configuration::schema())?;
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("configuration.schema.json");
    fs::write(output, format!("{schema}\n"))?;
    Ok(())
}
