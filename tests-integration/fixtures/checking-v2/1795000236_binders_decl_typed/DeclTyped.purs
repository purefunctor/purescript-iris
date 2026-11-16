module DeclTyped where
-- An equation whose binders are all irrefutable: toDecls.fromVarBinder drops the
-- TypedBinder annotation, so (x :: Int) is never checked against String.
f :: String -> String
f (x :: Int) = x
