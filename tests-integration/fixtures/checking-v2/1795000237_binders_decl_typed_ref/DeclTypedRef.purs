module DeclTypedRef where
-- Control for DeclTyped: a refutable second binder keeps the annotation (case path).
data B = T | F
f :: String -> B -> String
f (x :: Int) T = x
f x _ = x
