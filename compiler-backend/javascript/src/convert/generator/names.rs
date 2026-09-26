use std::borrow::Cow;
use std::iter;
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::{SmolStr, format_smolstr};

#[derive(Debug, Default)]
pub(super) struct NameAllocator {
    reserved: Rc<FxHashSet<SmolStr>>,
    names: FxHashSet<SmolStr>,
    /// The next suffix to probe for each normalized preferred name.
    ///
    /// Every smaller suffix is already reserved or allocated, and neither set
    /// ever shrinks, so resuming from here yields the same names as rescanning
    /// from `$1` without making repeated allocation quadratic.
    next_suffixes: FxHashMap<SmolStr, u32>,
}

impl NameAllocator {
    pub(super) fn with_reserved(reserved: Rc<FxHashSet<SmolStr>>) -> NameAllocator {
        NameAllocator { reserved, names: FxHashSet::default(), next_suffixes: FxHashMap::default() }
    }

    pub(super) fn allocate(&mut self, preferred: impl AsRef<str>) -> SmolStr {
        let mut normalized = normalize_identifier(preferred.as_ref());
        if identifier_is_reserved(&normalized) {
            normalized.to_mut().insert(0, '$');
        }
        let normalized = match normalized {
            Cow::Borrowed(name) => SmolStr::new(name),
            Cow::Owned(name) => SmolStr::from(name),
        };
        if self.claim(&normalized) {
            return normalized;
        }
        let mut suffix = self.next_suffixes.get(&normalized).copied().unwrap_or(1);
        let candidate = loop {
            let candidate = format_smolstr!("{normalized}${suffix}");
            suffix += 1;
            if self.claim(&candidate) {
                break candidate;
            }
        };
        self.next_suffixes.insert(normalized, suffix);
        candidate
    }

    fn claim(&mut self, candidate: &SmolStr) -> bool {
        !self.reserved.contains(candidate) && self.names.insert(candidate.clone())
    }

    pub(super) fn allocated_names(&self) -> impl Iterator<Item = &SmolStr> {
        iter::chain(self.reserved.iter(), self.names.iter())
    }
}

pub(crate) fn identifier_is_binding(identifier: &str) -> bool {
    identifier_has_valid_syntax(identifier) && !identifier_is_reserved(identifier)
}

fn normalize_identifier(preferred: &str) -> Cow<'_, str> {
    if identifier_has_valid_syntax(preferred) {
        return Cow::Borrowed(preferred);
    }

    let mut normalized = String::new();
    for (position, character) in preferred.chars().enumerate() {
        let valid_initial = character.is_ascii_alphabetic() || character == '_' || character == '$';
        let valid_subsequent = valid_initial || character.is_ascii_digit();
        if position == 0 && !valid_initial {
            normalized.push_str("value_");
        }
        if valid_subsequent {
            normalized.push(character);
        } else {
            normalized.push('_');
        }
    }
    if normalized.is_empty() {
        normalized.push_str("value");
    }
    Cow::Owned(normalized)
}

fn identifier_has_valid_syntax(identifier: &str) -> bool {
    let mut bytes = identifier.bytes();
    let Some(first) = bytes.next() else { return false };
    (first.is_ascii_alphabetic() || first == b'_' || first == b'$')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$')
}

fn identifier_is_reserved(identifier: &str) -> bool {
    matches!(
        identifier,
        "Array"
            | "Error"
            | "arguments"
            | "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "eval"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "implements"
            | "import"
            | "in"
            | "instanceof"
            | "interface"
            | "let"
            | "new"
            | "null"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "static"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
    )
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use rustc_hash::FxHashSet;
    use smol_str::{SmolStr, format_smolstr};

    use super::{NameAllocator, identifier_is_binding};

    fn reserved(names: &[&str]) -> Rc<FxHashSet<SmolStr>> {
        Rc::new(names.iter().map(SmolStr::new).collect())
    }

    #[test]
    fn repeated_allocation_numbers_suffixes_in_order() {
        let mut allocator = NameAllocator::default();
        assert_eq!(allocator.allocate("evidence"), "evidence");
        for suffix in 1..10_000 {
            assert_eq!(allocator.allocate("evidence"), format_smolstr!("evidence${suffix}"));
        }
        assert_eq!(allocator.next_suffixes[&SmolStr::new("evidence")], 10_000);
    }

    #[test]
    fn repeated_allocation_skips_reserved_and_preallocated_suffixes() {
        let mut allocator =
            NameAllocator::with_reserved(reserved(&["value", "value$1", "value$3"]));
        assert_eq!(allocator.allocate("value$5"), "value$5");
        assert_eq!(allocator.allocate("value"), "value$2");
        assert_eq!(allocator.allocate("value"), "value$4");
        assert_eq!(allocator.allocate("value"), "value$6");
        assert_eq!(allocator.allocate("value$7"), "value$7");
        assert_eq!(allocator.allocate("value"), "value$8");
        assert_eq!(allocator.allocate("value$7"), "value$7$1");
    }

    #[test]
    fn remembered_suffix_does_not_skip_names_allocated_through_other_paths() {
        let mut allocator = NameAllocator::default();
        assert_eq!(allocator.allocate("value"), "value");
        assert_eq!(allocator.allocate("value"), "value$1");
        assert_eq!(allocator.allocate("value$2"), "value$2");
        assert_eq!(allocator.allocate("value"), "value$3");
        assert_eq!(allocator.allocate("value$2"), "value$2$1");
        assert_eq!(allocator.allocate("value$2$1"), "value$2$1$1");
    }

    #[test]
    fn reserved_words_are_prefixed_before_suffixing() {
        let mut allocator = NameAllocator::default();
        assert_eq!(allocator.allocate("class"), "$class");
        assert_eq!(allocator.allocate("class"), "$class$1");
        assert_eq!(allocator.allocate("$class"), "$class$2");
        assert_eq!(allocator.allocate("class"), "$class$3");
    }

    #[test]
    fn normalized_names_share_one_suffix_sequence() {
        let mut allocator = NameAllocator::default();
        assert_eq!(allocator.allocate("foo-bar"), "foo_bar");
        assert_eq!(allocator.allocate("foo.bar"), "foo_bar$1");
        assert_eq!(allocator.allocate("foo_bar"), "foo_bar$2");
        assert_eq!(allocator.allocate("1"), "value_1");
        assert_eq!(allocator.allocate("value_1"), "value_1$1");
        assert_eq!(allocator.allocate(""), "value");
        assert_eq!(allocator.allocate(""), "value$1");
    }

    #[test]
    fn binding_syntax_rejects_invalid_and_reserved_names() {
        for (identifier, expected) in [
            ("value", true),
            ("value$1", true),
            ("_", true),
            ("$", true),
            ("", false),
            ("1value", false),
            ("foo-bar", false),
            ("é", false),
            ("class", false),
        ] {
            assert_eq!(identifier_is_binding(identifier), expected, "{identifier}");
        }
    }
}
