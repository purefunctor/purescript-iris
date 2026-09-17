mod id;
mod map;

pub use id::*;
pub use map::*;

use syntax::{SyntaxKind, SyntaxNode};

/// Allocates stable IDs for text ranges.
///
/// Text ranges are often used to annotate syntax nodes with positional
/// information; however, they're ill-suited for query-based compilers
/// designed to cache semantic information.
///
/// This algorithm allocates stable and type-safe IDs that can be used
/// for locating syntax nodes and associating information to text ranges.
///
/// A separate stabilization pass enables better caching behaviour for
/// algorithms like indexing and lowering, which used to have their own
/// stabilization passes as they traversed the CST.
pub fn stabilize_module(node: &SyntaxNode) -> StabilizedModule {
    let pointers: Vec<_> = node.preorder_pointers().collect();
    let mut ast_ptr_map = StabilizedModule::with_capacity(pointers.len());

    for pointer in pointers {
        if !matches!(
            pointer.kind(),
            SyntaxKind::Annotation | SyntaxKind::QualifiedName | SyntaxKind::LabelName
        ) {
            ast_ptr_map.allocate_ptr(pointer);
        }
    }

    ast_ptr_map
}
