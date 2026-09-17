use std::any;
use std::hash::BuildHasher;
use std::num::NonZeroU32;

use hashbrown::HashTable;
use rustc_hash::FxBuildHasher;
use syntax::ast::{AstNode, AstPtr};
use syntax::{SyntaxNode, SyntaxNodePtr};

use crate::AstId;

pub struct StabilizedModule {
    arena: Vec<SyntaxNodePtr>,
    table: HashTable<NonZeroU32>,
}

impl Default for StabilizedModule {
    fn default() -> StabilizedModule {
        let arena = vec![];
        let table = HashTable::default();
        StabilizedModule { arena, table }
    }
}

impl StabilizedModule {
    pub(crate) fn with_capacity(capacity: usize) -> StabilizedModule {
        let arena = Vec::with_capacity(capacity);
        let table = HashTable::with_capacity(capacity);
        StabilizedModule { arena, table }
    }

    pub fn allocate(&mut self, node: &SyntaxNode) {
        let ptr = SyntaxNodePtr::new(node);
        self.allocate_ptr(ptr);
    }

    pub(crate) fn allocate_ptr(&mut self, ptr: SyntaxNodePtr) {
        let hash = FxBuildHasher.hash_one(ptr);

        let id = {
            self.arena.push(ptr);
            let index = self.arena.len();
            // SAFETY: Vec::push ensures that the subsequent Vec::len
            // returns a non-zero value to be used as a 1-based index.
            unsafe { NonZeroU32::new_unchecked(index as u32) }
        };

        self.table.insert_unique(hash, id, |&id| arena_hasher(&self.arena, id));
    }

    pub fn lookup_cst<N: AstNode>(&self, cst: &N) -> Option<AstId<N>> {
        let ptr = AstPtr::new(cst);
        self.lookup_ptr(&ptr)
    }

    pub fn lookup_ptr<N: AstNode>(&self, ptr: &AstPtr<N>) -> Option<AstId<N>> {
        let ptr = ptr.syntax_node_ptr();
        let hash = FxBuildHasher.hash_one(ptr);
        self.table
            .find(hash, |&id| {
                let inner_ptr = arena_index(&self.arena, id).unwrap_or_else(|| {
                    unreachable!("invariant violated: {id} is not a valid index");
                });
                inner_ptr == ptr
            })
            .map(|&id| AstId::new(id))
    }

    pub fn ast_ptr<N: AstNode>(&self, id: AstId<N>) -> Option<AstPtr<N>> {
        self.syntax_ptr(id)?.cast()
    }

    pub fn syntax_ptr<N: AstNode>(&self, id: AstId<N>) -> Option<SyntaxNodePtr> {
        arena_index(&self.arena, id.id)
    }

    pub fn shrink_to_fit(&mut self) {
        self.arena.shrink_to_fit();
        self.table.shrink_to_fit(|&id| arena_hasher(&self.arena, id));
    }
}

impl PartialEq for StabilizedModule {
    fn eq(&self, other: &StabilizedModule) -> bool {
        self.arena == other.arena
    }
}

impl Eq for StabilizedModule {}

pub trait ExpectId<N>
where
    N: AstNode,
{
    fn expect_id(&self) -> AstId<N>;
}

impl<N: AstNode> ExpectId<N> for Option<AstId<N>> {
    #[inline]
    fn expect_id(&self) -> AstId<N> {
        self.unwrap_or_else(|| unreachable!("invariant violated: {}", any::type_name::<N>()))
    }
}

#[inline]
fn arena_index(arena: &[SyntaxNodePtr], id: NonZeroU32) -> Option<SyntaxNodePtr> {
    let index = id.get() as usize;
    arena.get(index - 1).copied()
}

#[inline]
fn arena_hasher(arena: &[SyntaxNodePtr], id: NonZeroU32) -> u64 {
    let ptr = arena_index(arena, id).unwrap_or_else(|| {
        unreachable!("invariant violated: {id} is not a valid index");
    });
    FxBuildHasher.hash_one(ptr)
}

#[cfg(test)]
mod tests {
    use syntax::ast::AstPtr;
    use syntax::{SyntaxKind, SyntaxNode, SyntaxNodePtr, SyntaxValue, TreeOwner, cst};

    use super::StabilizedModule;

    fn annotation(text_length: usize) -> SyntaxNode {
        let mut builder = syntree::Builder::new();
        let node = SyntaxValue::node(SyntaxKind::Annotation);
        builder.open(node).unwrap();
        let token = SyntaxValue::token(SyntaxKind::TEXT);
        builder.token(token, text_length).unwrap();
        builder.close().unwrap();
        let owner = TreeOwner::new(builder.build().unwrap());
        SyntaxNode::new_root(owner)
    }

    #[test]
    fn test_api() {
        let zero = annotation("ZERO".len());
        let one = annotation("ONE".len());

        // In revision 1, we only allocate zero
        let mut map_1 = StabilizedModule::default();
        map_1.allocate(&zero);

        let zero_ptr: AstPtr<cst::Annotation> = SyntaxNodePtr::new(&zero).cast().unwrap();
        assert!(
            map_1
                .lookup_ptr(&zero_ptr)
                .is_some_and(|id| map_1.ast_ptr(id).as_ref() == Some(&zero_ptr))
        );

        // In revision 2, we allocate zero and one
        let mut map_2 = StabilizedModule::default();
        map_2.allocate(&zero);
        map_2.allocate(&one);

        let one_ptr: AstPtr<cst::Annotation> = SyntaxNodePtr::new(&one).cast().unwrap();

        // Zero is valid in revision 2
        assert!(
            map_2
                .lookup_ptr(&zero_ptr)
                .is_some_and(|id| map_2.ast_ptr(id).as_ref() == Some(&zero_ptr))
        );

        // One is valid in revision 2
        assert!(
            map_2
                .lookup_ptr(&one_ptr)
                .is_some_and(|id| map_2.ast_ptr(id).as_ref() == Some(&one_ptr))
        );

        // One is invalid in revision 1.
        assert!(map_1.lookup_ptr(&one_ptr).is_none());
    }

    #[test]
    fn test_equality() {
        let zero = annotation("ZERO".len());
        let one = annotation("ONE".len());
        let two = annotation("TWO TWO".len());

        {
            let mut map_a = StabilizedModule::default();
            let mut map_b = StabilizedModule::default();
            let mut map_c = StabilizedModule::default();

            map_a.allocate(&zero);
            map_b.allocate(&zero);
            map_c.allocate(&zero);

            // Symmetric
            assert!(map_a == map_b);
            assert!(map_b == map_a);

            // Transitive
            assert!(map_b == map_c);
            assert!(map_a == map_c);
        }

        {
            let mut map_a = StabilizedModule::default();
            let mut map_b = StabilizedModule::default();

            map_a.allocate(&zero);
            map_a.allocate(&two);

            map_b.allocate(&one);
            map_b.allocate(&two);

            // Symmetric
            assert!(map_a != map_b);
            assert!(!(map_a == map_b));
        }

        {
            let mut map_a = StabilizedModule::default();
            let mut map_b = StabilizedModule::default();

            map_a.allocate(&zero);
            map_b.allocate(&one);
            map_b.allocate(&two);

            // Length check
            assert!(map_a != map_b);
        }

        {
            let mut map_a = StabilizedModule::default();
            let mut map_b = StabilizedModule::default();

            map_a.allocate(&zero);
            map_a.allocate(&one);

            map_b.allocate(&one);
            map_b.allocate(&zero);

            // Index check
            assert!(map_a != map_b);
        }
    }
}
