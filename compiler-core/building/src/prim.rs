use files::Files;
use prim_constants::MODULE_MAP;

use crate::QueryEngine;

pub const SCHEME: &str = "prim";

pub fn configure(engine: &mut QueryEngine, files: &mut Files) {
    for (name, content) in MODULE_MAP {
        let path = format!("{SCHEME}://localhost/{name}.purs");
        let id = files.insert(path, *content);

        engine.set_content(id, *content);
        engine.set_dialect(id, files.dialect(id));
        engine.set_module_file(name, id);
    }
}
