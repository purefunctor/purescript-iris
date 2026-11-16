macro_rules! create_cst_struct {
    ($($kind:ident),* $(,)?) => {
        $(
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub struct $kind {
                node: crate::SyntaxNode,
            }

            impl crate::ast::AstNode for $kind {
                fn can_cast(kind: crate::SyntaxKind) -> bool
                where
                    Self: Sized,
                {
                    matches!(kind, crate::SyntaxKind::$kind)
                }

                fn cast(node: crate::SyntaxNode) -> Option<Self>
                where
                    Self: Sized,
                {
                    if Self::can_cast(node.kind()) {
                        Some(Self { node })
                    } else {
                        None
                    }
                }

                fn syntax(&self) -> &crate::SyntaxNode {
                    &self.node
                }
            }
        )+
    };
}

macro_rules! create_cst_enum {
    ($kind:ident | $key_0:ident$(|$key:ident)*) => {
        create_cst_struct!($key_0);
        $(
            create_cst_struct!($key);
        )*

        create_cst_union!($kind | $key_0 $(| $key)*);
    };
}

macro_rules! create_cst_union {
    ($kind:ident | $key_0:ident$(|$key:ident)*) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $kind {
            $key_0($key_0),
            $(
                $key($key),
            )*
        }

        impl crate::ast::AstNode for $kind {
            fn can_cast(kind: crate::SyntaxKind) -> bool
            where
                Self: Sized,
            {
                $key_0::can_cast(kind) $(|| $key::can_cast(kind))*
            }

            fn cast(node: crate::SyntaxNode) -> Option<Self>
            where
                Self: Sized,
            {
                let kind = node.kind();
                if $key_0::can_cast(kind) {
                    Some($kind::$key_0($key_0 { node }))
                } $(else if $key::can_cast(kind) {
                    Some($kind::$key($key { node }))
                })* else {
                    None
                }
            }

            fn syntax(&self) -> &crate::SyntaxNode {
                match self {
                    $kind::$key_0(t) => t.syntax(),
                    $(
                        $kind::$key(t) => t.syntax(),
                    )*
                }
            }
        }
    };
}

macro_rules! has_token {
    ($kind:ident $(|$name:ident() -> $token:ident)+) => {
        impl $kind {
            $(
                pub fn $name(&self) -> Option<crate::SyntaxToken> {
                    crate::ast::support::token(self.syntax(), crate::SyntaxKind::$token)
                }
            )+
        }
    };
}

macro_rules! has_token_set {
    ($kind:ident $(|$name:ident() -> $set:expr)+) => {
        impl $kind {
            $(
                pub fn $name(&self) -> Option<crate::SyntaxToken> {
                    self.syntax().token_in_set($set)
                }
            )+
        }
    };
}

macro_rules! has_tokens {
    ($kind:ident $(|$name:ident() -> $token:ident)+) => {
        impl $kind {
            $(
                pub fn $name(&self) -> impl Iterator<Item = crate::SyntaxToken> {
                    self.syntax().children_with_tokens().filter_map(|n| {
                        let t = n.into_token()?;
                        if t.kind() == crate::SyntaxKind::$token {
                            Some(t)
                        } else {
                            None
                        }
                    })
                }
            )+
        }
    };
}

macro_rules! has_child {
    ($kind:ident $(|$name:ident() -> $child:ident)+) => {
        impl $kind {
            $(
                pub fn $name(&self) -> Option<crate::cst::$child> {
                    crate::ast::support::child(self.syntax())
                }
            )+
        }
    };
}

macro_rules! has_children {
    ($kind:ident $(|$name:ident() -> $child:ident)+) => {
        impl $kind {
            $(
                pub fn $name(&self) -> crate::ast::AstChildren<crate::cst::$child> {
                    crate::ast::support::children(self.syntax())
                }
            )+
        }
    };
}
