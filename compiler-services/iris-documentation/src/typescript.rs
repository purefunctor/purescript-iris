use std::path::PathBuf;

use ts_rs::{Config as TypeScriptExportConfig, TS};

use crate::{Error, schema};

pub fn export_typescript(output: PathBuf) -> Result<(), Error> {
    let typescript = TypeScriptExportConfig::new().with_out_dir(output);

    schema::Package::export_all(&typescript)?;
    schema::Module::export_all(&typescript)?;

    Ok(())
}
